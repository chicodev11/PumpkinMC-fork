use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::Sound;
use rand::RngExt;

use super::goat::GoatEntity;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainState, BrainTick,
    activity::ActivityData,
    behavior::{
        AnimalMakeLove, AnimalPanic, CountDownCooldownTicks, DoNothing, FollowTemptation,
        LookAtTargetSink, MoveToTargetSink, PrepareRamNearestTarget, RamTarget, Swim, Timed,
        baby_follow_adult,
        long_jump::{
            LongJumpMidJump, LongJumpToRandomPos, TimeBetweenLongJumps,
            default_acceptable_landing_spot,
        },
        ram_target::TimeBetweenRams,
        random_stroll, run_one, set_entity_look_target_sometimes, set_walk_target_from_look_target,
    },
    memory::{MemoryStatus, PackedMemories, types},
    sensing::SensorType,
};
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::mob::Mob;
use crate::world::World;

pub const RAM_PREPARE_TIME: i64 = 20;
pub const RAM_MAX_DISTANCE: i32 = 7;
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 2.0;
const SPEED_MULTIPLIER_WHEN_PREPARING_TO_RAM: f32 = 1.25;
const TIME_BETWEEN_LONG_JUMPS: TimeBetweenLongJumps = (600, 1200);
pub const MAX_LONG_JUMP_HEIGHT: i32 = 5;
pub const MAX_LONG_JUMP_WIDTH: i32 = 5;
pub const MAX_JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;
const TIME_BETWEEN_RAMS: TimeBetweenRams = (600, 6000);
const TIME_BETWEEN_RAMS_SCREAMER: TimeBetweenRams = (100, 300);
const SPEED_MULTIPLIER_WHEN_RAMMING: f32 = 3.0;
pub const RAM_MIN_DISTANCE: i32 = 4;
pub const ADULT_RAM_KNOCKBACK_FORCE: f64 = 2.5;
pub const BABY_RAM_KNOCKBACK_FORCE: f64 = 1.0;

pub(super) fn make_brain(goat: &GoatEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &[],
        sensor_types: &[
            SensorType::NearestLivingEntities,
            SensorType::NearestPlayers,
            SensorType::NearestItems,
            SensorType::NearestAdult,
            SensorType::HurtBy,
            SensorType::FoodTemptations,
        ],
        activities,
    }
    .make_brain(goat, packed)
}

/// Vanilla `GoatAi.initMemories`.
pub(super) fn init_memories(tick: &mut BrainTick<'_>) {
    let mut random = tick.mob.get_random();
    let long_jump = random.random_range(TIME_BETWEEN_LONG_JUMPS.0..=TIME_BETWEEN_LONG_JUMPS.1);
    let ram = random.random_range(TIME_BETWEEN_RAMS.0..=TIME_BETWEEN_RAMS.1);
    drop(random);
    tick.state.set(types::LONG_JUMP_COOLDOWN_TICKS, long_jump);
    tick.state.set(types::RAM_COOLDOWN_TICKS, ram);
}

fn is_screaming(mob: &dyn Mob) -> bool {
    mob.cast_any()
        .downcast_ref::<GoatEntity>()
        .is_some_and(GoatEntity::is_screaming)
}

fn screaming_sound(mob: &dyn Mob, regular: Sound, screaming: Sound) -> Sound {
    if is_screaming(mob) {
        screaming
    } else {
        regular
    }
}

fn time_between_rams(mob: &dyn Mob) -> TimeBetweenRams {
    if is_screaming(mob) {
        TIME_BETWEEN_RAMS_SCREAMER
    } else {
        TIME_BETWEEN_RAMS
    }
}

/// Vanilla `GoatAi.RAM_TARGET_CONDITIONS`: anything but goats, armor stands only with mob
/// griefing, and only inside the world border.
fn ram_target_conditions() -> TargetPredicate {
    let mut predicate = TargetPredicate::create_attackable();
    predicate.set_predicate(|target, world: &World| {
        target.entity.entity_type != &EntityType::GOAT
            && (world.level_info.load().game_rules.mob_griefing
                || target.entity.entity_type != &EntityType::ARMOR_STAND)
            && world
                .worldborder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_within_bounds_box(&target.entity.bounding_box.load())
    });
    predicate
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_long_jump_activity(),
        init_ram_activity(),
    ]
}

fn init_core_activity() -> ActivityData {
    ActivityData::with_priority_start(
        Activity::Core,
        0,
        vec![
            Box::new(Timed::new(Swim::new(0.8))),
            Box::new(Timed::new(AnimalPanic::new(
                SPEED_MULTIPLIER_WHEN_PANICKING,
            ))),
            Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
            Box::new(Timed::new(MoveToTargetSink::default())),
            Box::new(CountDownCooldownTicks::new(
                types::TEMPTATION_COOLDOWN_TICKS,
            )),
            Box::new(CountDownCooldownTicks::new(types::LONG_JUMP_COOLDOWN_TICKS)),
            Box::new(CountDownCooldownTicks::new(types::RAM_COOLDOWN_TICKS)),
        ],
    )
}

fn init_idle_activity() -> ActivityData {
    ActivityData::with_pairs_and_conditions(
        Activity::Idle,
        vec![
            (
                0,
                Box::new(set_entity_look_target_sometimes::create_matching(
                    Some(&EntityType::PLAYER),
                    6.0,
                    30,
                    60,
                )),
            ),
            (
                0,
                Box::new(Timed::new(AnimalMakeLove::new(&EntityType::GOAT))),
            ),
            (
                1,
                Box::new(Timed::new(FollowTemptation::new(
                    |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                    |_| {
                        crate::entity::ai::brain::behavior::follow_temptation::DEFAULT_CLOSE_ENOUGH_DIST
                    },
                ))),
            ),
            (
                2,
                Box::new(baby_follow_adult::create(
                    ADULT_FOLLOW_RANGE.0,
                    ADULT_FOLLOW_RANGE.1,
                    SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                )),
            ),
            (
                3,
                Box::new(run_one(vec![
                    (
                        Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                        2,
                    ),
                    (
                        Box::new(set_walk_target_from_look_target::create(
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            3,
                        )),
                        2,
                    ),
                    (Box::new(DoNothing::new(30, 60)), 1),
                ])),
            ),
        ],
        vec![
            (types::RAM_COOLDOWN_TICKS.id(), MemoryStatus::ValuePresent),
            (
                types::LONG_JUMP_COOLDOWN_TICKS.id(),
                MemoryStatus::ValuePresent,
            ),
        ],
    )
}

fn init_long_jump_activity() -> ActivityData {
    ActivityData::with_pairs_and_conditions(
        Activity::LongJump,
        vec![
            (
                0,
                Box::new(Timed::new(LongJumpMidJump::new(
                    TIME_BETWEEN_LONG_JUMPS,
                    Sound::EntityGoatStep,
                ))),
            ),
            (
                1,
                Box::new(Timed::new(LongJumpToRandomPos::new(
                    TIME_BETWEEN_LONG_JUMPS,
                    MAX_LONG_JUMP_HEIGHT,
                    MAX_LONG_JUMP_WIDTH,
                    MAX_JUMP_VELOCITY_MULTIPLIER,
                    |mob| {
                        screaming_sound(
                            mob,
                            Sound::EntityGoatLongJump,
                            Sound::EntityGoatScreamingLongJump,
                        )
                    },
                    default_acceptable_landing_spot,
                    None,
                ))),
            ),
        ],
        vec![
            (types::TEMPTING_PLAYER.id(), MemoryStatus::ValueAbsent),
            (types::BREED_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (
                types::LONG_JUMP_COOLDOWN_TICKS.id(),
                MemoryStatus::ValueAbsent,
            ),
        ],
    )
}

fn init_ram_activity() -> ActivityData {
    ActivityData::with_pairs_and_conditions(
        Activity::Ram,
        vec![
            (
                0,
                Box::new(Timed::new(RamTarget::new(
                    time_between_rams,
                    ram_target_conditions(),
                    SPEED_MULTIPLIER_WHEN_RAMMING,
                    |mob| {
                        if mob.get_mob_entity().living_entity.is_baby() {
                            BABY_RAM_KNOCKBACK_FORCE
                        } else {
                            ADULT_RAM_KNOCKBACK_FORCE
                        }
                    },
                    |mob| {
                        screaming_sound(
                            mob,
                            Sound::EntityGoatRamImpact,
                            Sound::EntityGoatScreamingRamImpact,
                        )
                    },
                    |_| Sound::EntityGoatHornBreak,
                    |mob| {
                        mob.cast_any()
                            .downcast_ref::<GoatEntity>()
                            .is_some_and(GoatEntity::drop_horn)
                    },
                ))),
            ),
            (
                1,
                Box::new(Timed::new(PrepareRamNearestTarget::new(
                    |mob| time_between_rams(mob).0,
                    RAM_MIN_DISTANCE,
                    RAM_MAX_DISTANCE,
                    SPEED_MULTIPLIER_WHEN_PREPARING_TO_RAM,
                    ram_target_conditions(),
                    RAM_PREPARE_TIME,
                    |mob| {
                        screaming_sound(
                            mob,
                            Sound::EntityGoatPrepareRam,
                            Sound::EntityGoatScreamingPrepareRam,
                        )
                    },
                ))),
            ),
        ],
        vec![
            (types::TEMPTING_PLAYER.id(), MemoryStatus::ValueAbsent),
            (types::BREED_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::RAM_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

/// Vanilla `GoatAi.updateActivity`.
pub(super) fn update_activity(state: &mut BrainState) {
    state.set_active_activity_to_first_valid(&[Activity::Ram, Activity::LongJump, Activity::Idle]);
}
