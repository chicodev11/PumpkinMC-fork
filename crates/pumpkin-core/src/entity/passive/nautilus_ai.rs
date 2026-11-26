use std::sync::Arc;

use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use rand::RngExt;

use super::nautilus::NautilusEntity;
use crate::entity::EntityBase;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainState, BrainTick,
    activity::ActivityData,
    behavior::{
        AnimalMakeLove, AnimalPanic, ChargeAttack, CountDownCooldownTicks, FollowTemptation,
        LookAtTargetSink, MoveToTargetSink, Timed, gate, random_stroll,
        set_walk_target_from_look_target, start_attacking, utils,
    },
    memory::{MemoryModuleId, MemoryStatus, PackedMemories, types},
    sensing::{SensorType, is_entity_attackable_ignoring_line_of_sight},
};
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::mob::Mob;
use crate::world::World;

const SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.3;
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f32 = 0.4;
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 1.6;
const TIME_BETWEEN_NON_PLAYER_ATTACKS: (i32, i32) = (2400, 3600);
const SPEED_WHEN_ATTACKING: f32 = 0.6;
const ATTACK_KNOCKBACK_FORCE: f32 = 2.0;
const ANGER_DURATION: i64 = 400;
const TIME_BETWEEN_ATTACKS: i32 = 80;
const MAX_CHARGE_DISTANCE: f64 = 12.0;
const MAX_TARGET_DETECTION_DISTANCE: f64 = 11.0;
const MEMORY_TYPES: [MemoryModuleId; 2] =
    [types::ANGRY_AT.id(), types::ATTACK_TARGET_COOLDOWN.id()];

const SENSOR_TYPES: [SensorType; 5] = [
    SensorType::NearestLivingEntities,
    SensorType::NearestAdult,
    SensorType::NearestPlayers,
    SensorType::HurtBy,
    SensorType::NautilusTemptations,
];
const ZOMBIE_SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 0.9;
const ZOMBIE_SPEED_WHEN_ATTACKING: f32 = 0.5;

pub(super) fn make_brain(nautilus: &NautilusEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &MEMORY_TYPES,
        sensor_types: &SENSOR_TYPES,
        activities,
    }
    .make_brain(nautilus, packed)
}

/// Vanilla `ZombieNautilus.BRAIN_PROVIDER`.
pub(super) fn make_zombie_brain(nautilus: &NautilusEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &MEMORY_TYPES,
        sensor_types: &SENSOR_TYPES,
        activities: zombie_activities,
    }
    .make_brain(nautilus, packed)
}

fn temptation_distance(mob: &dyn Mob) -> f64 {
    if mob.get_mob_entity().living_entity.is_baby() {
        2.5
    } else {
        3.5
    }
}

fn idle_stroll() -> gate::GateBehavior {
    gate::GateBehavior::new(
        "GateBehavior",
        vec![(types::WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        Vec::new(),
        gate::OrderPolicy::Ordered,
        gate::RunningPolicy::TryAll,
        vec![
            (
                Box::new(random_stroll::swim(SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER)),
                2,
            ),
            (
                Box::new(set_walk_target_from_look_target::create(
                    SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER,
                    3,
                )),
                3,
            ),
        ],
    )
}

fn fight_activity(speed: f32, charge_sound: Sound) -> ActivityData {
    ActivityData::with_pairs_and_conditions(
        Activity::Fight,
        vec![(
            0,
            Box::new(Timed::new(ChargeAttack::new(
                TIME_BETWEEN_ATTACKS,
                attack_target_conditions(),
                speed,
                ATTACK_KNOCKBACK_FORCE,
                MAX_CHARGE_DISTANCE,
                MAX_TARGET_DETECTION_DISTANCE,
                charge_sound,
            ))),
        )],
        vec![
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (types::TEMPTING_PLAYER.id(), MemoryStatus::ValueAbsent),
            (types::BREED_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::CHARGE_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

/// Vanilla `ZombieNautilusAi.getActivities`: no panic and no breeding.
fn zombie_activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
                Box::new(CountDownCooldownTicks::new(
                    types::TEMPTATION_COOLDOWN_TICKS,
                )),
                Box::new(CountDownCooldownTicks::new(types::CHARGE_COOLDOWN_TICKS)),
                Box::new(CountDownCooldownTicks::new(types::ATTACK_TARGET_COOLDOWN)),
            ],
        ),
        ActivityData::with_pairs(
            Activity::Idle,
            vec![
                (
                    1,
                    Box::new(Timed::new(FollowTemptation::new(
                        |_| ZOMBIE_SPEED_MULTIPLIER_WHEN_TEMPTED,
                        temptation_distance,
                    ))),
                ),
                (
                    2,
                    Box::new(start_attacking::create_with_mut_finder(
                        |_| true,
                        find_nearest_valid_attack_target,
                    )),
                ),
                (3, Box::new(idle_stroll())),
            ],
        ),
        fight_activity(ZOMBIE_SPEED_WHEN_ATTACKING, Sound::EntityZombieNautilusDash),
    ]
}

/// Vanilla `NautilusAi.initMemories`.
pub(super) fn init_memories(tick: &mut BrainTick<'_>) {
    let cooldown = tick
        .mob
        .get_random()
        .random_range(TIME_BETWEEN_NON_PLAYER_ATTACKS.0..=TIME_BETWEEN_NON_PLAYER_ATTACKS.1);
    tick.state.set(types::ATTACK_TARGET_COOLDOWN, cooldown);
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(AnimalPanic::new(
                    SPEED_MULTIPLIER_WHEN_PANICKING,
                ))),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
                Box::new(CountDownCooldownTicks::new(
                    types::TEMPTATION_COOLDOWN_TICKS,
                )),
                Box::new(CountDownCooldownTicks::new(types::CHARGE_COOLDOWN_TICKS)),
                Box::new(CountDownCooldownTicks::new(types::ATTACK_TARGET_COOLDOWN)),
            ],
        ),
        ActivityData::with_pairs(
            Activity::Idle,
            vec![
                (
                    1,
                    Box::new(Timed::new(
                        AnimalMakeLove::new(&EntityType::NAUTILUS)
                            .with_speed(SPEED_MULTIPLIER_WHEN_MAKING_LOVE, 2),
                    )),
                ),
                (
                    2,
                    Box::new(Timed::new(FollowTemptation::new(
                        |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                        temptation_distance,
                    ))),
                ),
                (
                    3,
                    Box::new(start_attacking::create_with_mut_finder(
                        |_| true,
                        find_nearest_valid_attack_target,
                    )),
                ),
                (4, Box::new(idle_stroll())),
            ],
        ),
        fight_activity(SPEED_WHEN_ATTACKING, Sound::EntityNautilusDash),
    ]
}

/// Vanilla `NautilusAi.ATTACK_TARGET_CONDITIONS`: combat targeting that spares armor stands
/// unless mob griefing is on, and stays inside the world border.
fn attack_target_conditions() -> TargetPredicate {
    let mut predicate = TargetPredicate::create_attackable();
    predicate.set_predicate(|target, world: &World| {
        (world.level_info.load().game_rules.mob_griefing
            || target.entity.entity_type != &EntityType::ARMOR_STAND)
            && world
                .worldborder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_within_bounds_box(&target.entity.bounding_box.load())
    });
    predicate
}

/// Vanilla `NautilusAi.findNearestValidAttackTarget`.
fn find_nearest_valid_attack_target(tick: &mut BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
    let nautilus = tick.mob.cast_any().downcast_ref::<NautilusEntity>()?;
    let entity = tick.mob.get_entity();
    if utils::is_breeding(tick.state)
        || !entity.is_in_water()
        || nautilus.mob_entity.living_entity.is_baby()
        || nautilus.is_tame()
    {
        return None;
    }
    let ctx = tick.visibility();
    if let Some(angry_at) =
        utils::get_living_entity_from_uuid_memory(tick.state, tick.world, types::ANGRY_AT).filter(
            |target| {
                target.get_entity().is_in_water()
                    && is_entity_attackable_ignoring_line_of_sight(&ctx, target.as_ref())
            },
        )
    {
        return Some(angry_at);
    }
    if tick
        .state
        .has_memory_value(types::ATTACK_TARGET_COOLDOWN.id())
    {
        return None;
    }
    init_memories(tick);
    if tick.mob.get_random().random::<f32>() < 0.5 {
        return None;
    }
    let ctx = tick.visibility();
    tick.state
        .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)?
        .find_closest(&ctx, |target| is_hostile_target(target.as_ref()))
}

/// Vanilla `NautilusAi.setAngerTarget`.
pub(super) fn set_anger_target(tick: &mut BrainTick<'_>, attacker: &dyn EntityBase) {
    if is_entity_attackable_ignoring_line_of_sight(&tick.visibility(), attacker) {
        tick.state.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
        tick.state.set_with_expiry(
            types::ANGRY_AT,
            attacker.get_entity().entity_uuid,
            ANGER_DURATION,
        );
    }
}

fn is_hostile_target(target: &dyn EntityBase) -> bool {
    let entity = target.get_entity();
    entity.is_in_water()
        && entity
            .entity_type
            .has_tag(&tag::EntityType::MINECRAFT_NAUTILUS_HOSTILES)
}

/// Vanilla `NautilusAi.updateActivity`.
pub(super) fn update_activity(state: &mut BrainState) {
    state.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
}
