use std::sync::Arc;

use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityPose, EntityStatus};
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::Sound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use super::warden::WardenEntity;
use crate::entity::EntityBase;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainState, BrainTick,
    activity::ActivityData,
    behavior::{
        Behavior, DoNothing, LookAtTargetSink, MoveToTargetSink, OneShot, Swim, Timed,
        go_to_target_location, interact::set_entity_look_target, melee_attack, random_stroll,
        run_one_with_conditions, set_walk_target_from_attack_target_if_target_out_of_reach,
        stop_attacking_if_target_invalid, utils,
    },
    memory::{
        MemoryModuleId, MemoryStatus, PackedMemories,
        position_tracker::{BlockPosTracker, PositionTracker},
        types,
    },
    sensing::SensorType,
};
use crate::entity::mob::Mob;

const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.5;
const SPEED_MULTIPLIER_WHEN_INVESTIGATING: f32 = 0.7;
const SPEED_MULTIPLIER_WHEN_FIGHTING: f32 = 1.2;
const MELEE_ATTACK_COOLDOWN: i64 = 18;
const DIGGING_DURATION: i32 = 100;
pub const EMERGE_DURATION: i32 = 134;
pub const ROAR_DURATION: i32 = 84;
const SNIFFING_DURATION: i32 = 84;
pub const DIGGING_COOLDOWN: i64 = 1200;
const DISTURBANCE_LOCATION_EXPIRY_TIME: i64 = 100;
/// Vanilla `SonicBoom.COOLDOWN`.
pub const SONIC_BOOM_COOLDOWN: i64 = 40;
const MEMORY_TYPES: [MemoryModuleId; 4] = [
    types::NEAREST_VISIBLE_NEMESIS.id(),
    types::RECENT_PROJECTILE.id(),
    types::TOUCH_COOLDOWN.id(),
    types::VIBRATION_COOLDOWN.id(),
];

pub(super) fn make_brain(warden: &WardenEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &MEMORY_TYPES,
        sensor_types: &[SensorType::NearestPlayers, SensorType::WardenEntity],
        activities,
    }
    .make_brain(warden, packed)
}

fn warden<'a>(tick: &BrainTick<'a>) -> Option<&'a WardenEntity> {
    tick.mob.cast_any().downcast_ref::<WardenEntity>()
}

fn set_pose(tick: &BrainTick<'_>, pose: EntityPose) {
    tick.mob.get_entity().set_pose(pose);
}

fn has_pose(tick: &BrainTick<'_>, pose: EntityPose) -> bool {
    tick.mob.get_entity().pose.load() == pose
}

fn play_sound(tick: &BrainTick<'_>, sound: Sound, volume: f32, pitch: f32) {
    tick.mob.play_entity_sound(sound, volume, pitch);
}

fn closer_than(tick: &BrainTick<'_>, target: &dyn EntityBase, xz: f64, y: f64) -> bool {
    let pos = tick.mob.get_entity().pos.load();
    let other = target.get_entity().pos.load();
    let (dx, dy, dz) = (other.x - pos.x, other.y - pos.y, other.z - pos.z);
    dx * dx + dz * dz < xz * xz && dy * dy < y * y
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(Swim::new(0.8))),
                Box::new(set_warden_look_target()),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
            ],
        ),
        ActivityData::with_gate_memory(
            Activity::Emerge,
            5,
            vec![Box::new(Timed::new(Emerging))],
            types::IS_EMERGING.id(),
        ),
        ActivityData::with_pairs_and_conditions(
            Activity::Dig,
            vec![
                (0, Box::new(Timed::new(ForceUnmount))),
                (1, Box::new(Timed::new(Digging))),
            ],
            vec![
                (types::ROAR_TARGET.id(), MemoryStatus::ValueAbsent),
                (types::DIG_COOLDOWN.id(), MemoryStatus::ValueAbsent),
            ],
        ),
        ActivityData::with_priority_start(
            Activity::Idle,
            10,
            vec![
                Box::new(set_roar_target()),
                Box::new(try_to_sniff()),
                Box::new(run_one_with_conditions(
                    vec![(types::IS_SNIFFING.id(), MemoryStatus::ValueAbsent)],
                    vec![
                        (
                            Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                            2,
                        ),
                        (Box::new(DoNothing::new(30, 60)), 1),
                    ],
                )),
            ],
        ),
        ActivityData::with_gate_memory(
            Activity::Investigate,
            5,
            vec![
                Box::new(set_roar_target()),
                Box::new(go_to_target_location::create(
                    types::DISTURBANCE_LOCATION,
                    2,
                    SPEED_MULTIPLIER_WHEN_INVESTIGATING,
                )),
            ],
            types::DISTURBANCE_LOCATION.id(),
        ),
        ActivityData::with_gate_memory(
            Activity::Sniff,
            5,
            vec![Box::new(set_roar_target()), Box::new(Timed::new(Sniffing))],
            types::IS_SNIFFING.id(),
        ),
        ActivityData::with_gate_memory(
            Activity::Roar,
            10,
            vec![Box::new(Timed::new(Roar))],
            types::ROAR_TARGET.id(),
        ),
        init_fight_activity(),
    ]
}

/// Vanilla `WardenAi.initFightActivity`.
fn init_fight_activity() -> ActivityData {
    ActivityData::with_gate_memory(
        Activity::Fight,
        10,
        vec![
            Box::new(dig_cooldown_setter()),
            Box::new(stop_attacking_if_target_invalid::create_with(
                |tick, target| {
                    warden(tick).is_none_or(|warden| {
                        !warden.anger_level_in(tick.state).is_angry()
                            || !warden.can_target_entity(target.as_ref())
                    })
                },
                on_target_invalid,
                false,
            )),
            Box::new(set_entity_look_target::create_with_state(
                |state, entity| {
                    state.get(types::ATTACK_TARGET).is_some_and(|target| {
                        target.get_entity().entity_id == entity.get_entity().entity_id
                    })
                },
                // Vanilla reads the follow range once, when the activity is built.
                WardenEntity::FOLLOW_RANGE,
            )),
            Box::new(
                set_walk_target_from_attack_target_if_target_out_of_reach::create(
                    SPEED_MULTIPLIER_WHEN_FIGHTING,
                ),
            ),
            Box::new(Timed::new(SonicBoom)),
            Box::new(melee_attack::create(MELEE_ATTACK_COOLDOWN)),
        ],
        types::ATTACK_TARGET.id(),
    )
}

/// Vanilla `WardenAi.updateActivity`.
pub(super) fn update_activity(state: &mut BrainState) {
    state.set_active_activity_to_first_valid(&[
        Activity::Emerge,
        Activity::Dig,
        Activity::Roar,
        Activity::Fight,
        Activity::Investigate,
        Activity::Sniff,
        Activity::Idle,
    ]);
}

/// Vanilla `WardenAi.onTargetInvalid`.
fn on_target_invalid(tick: &mut BrainTick<'_>, target: &Arc<dyn EntityBase>) {
    if let Some(warden) = warden(tick)
        && !warden.can_target_entity(target.as_ref())
    {
        warden.clear_anger(target.as_ref());
    }
    set_dig_cooldown(tick.state);
}

/// Vanilla `WardenAi.setDigCooldown`: only refreshes a cooldown that is already running.
pub fn set_dig_cooldown(state: &mut BrainState) {
    if state.has_memory_value(types::DIG_COOLDOWN.id()) {
        state.set_with_expiry(types::DIG_COOLDOWN, (), DIGGING_COOLDOWN);
    }
}

/// Vanilla `WardenAi.setDisturbanceLocation`.
pub fn set_disturbance_location(warden: &WardenEntity, state: &mut BrainState, pos: BlockPos) {
    let world = warden.get_entity().world.load();
    let within_border = world
        .worldborder
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_block(pos.0.x, pos.0.z);
    if !within_border
        || warden.get_entity_angry_at_in(state).is_some()
        || state.has_memory_value(types::ATTACK_TARGET.id())
    {
        return;
    }
    set_dig_cooldown(state);
    state.set_with_expiry(types::SNIFF_COOLDOWN, (), DISTURBANCE_LOCATION_EXPIRY_TIME);
    state.set_with_expiry(
        types::LOOK_TARGET,
        Arc::new(BlockPosTracker::new(pos)) as Arc<dyn PositionTracker>,
        DISTURBANCE_LOCATION_EXPIRY_TIME,
    );
    state.set_with_expiry(
        types::DISTURBANCE_LOCATION,
        pos,
        DISTURBANCE_LOCATION_EXPIRY_TIME,
    );
    state.erase(types::WALK_TARGET.id());
}

/// Vanilla `WardenAi.DIG_COOLDOWN_SETTER`.
fn dig_cooldown_setter() -> OneShot {
    OneShot::new(
        "DigCooldownSetter",
        vec![(types::DIG_COOLDOWN.id(), MemoryStatus::Registered)],
        |tick| {
            set_dig_cooldown(tick.state);
            true
        },
    )
}

/// Vanilla `SetWardenLookTarget`.
fn set_warden_look_target() -> OneShot {
    OneShot::new(
        "SetWardenLookTarget",
        vec![
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::DISTURBANCE_LOCATION.id(), MemoryStatus::Registered),
            (types::ROAR_TARGET.id(), MemoryStatus::Registered),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValueAbsent),
        ],
        |tick| {
            let Some(pos) = tick
                .state
                .get(types::ROAR_TARGET)
                .map(|target| target.get_entity().block_pos.load())
                .or_else(|| tick.state.get(types::DISTURBANCE_LOCATION).copied())
            else {
                return false;
            };
            tick.state.set(
                types::LOOK_TARGET,
                Arc::new(BlockPosTracker::new(pos)) as Arc<dyn PositionTracker>,
            );
            true
        },
    )
}

/// Vanilla `SetRoarTarget.create(Warden::getEntityAngryAt)`.
fn set_roar_target() -> OneShot {
    OneShot::new(
        "SetRoarTarget",
        vec![
            (types::ROAR_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValueAbsent),
            (
                types::CANT_REACH_WALK_TARGET_SINCE.id(),
                MemoryStatus::Registered,
            ),
        ],
        |tick| {
            let Some(warden) = warden(tick) else {
                return false;
            };
            let Some(target) = warden
                .get_entity_angry_at_in(tick.state)
                .filter(|target| warden.can_target_entity(target.as_ref()))
            else {
                return false;
            };
            tick.state.set(types::ROAR_TARGET, target);
            tick.state.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
            true
        },
    )
}

/// Vanilla `TryToSniff`.
fn try_to_sniff() -> OneShot {
    OneShot::new(
        "TryToSniff",
        vec![
            (types::IS_SNIFFING.id(), MemoryStatus::Registered),
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (types::SNIFF_COOLDOWN.id(), MemoryStatus::ValueAbsent),
            (types::NEAREST_ATTACKABLE.id(), MemoryStatus::ValuePresent),
            (types::DISTURBANCE_LOCATION.id(), MemoryStatus::ValueAbsent),
        ],
        |tick| {
            tick.state.set(types::IS_SNIFFING, ());
            let cooldown = tick.mob.get_random().random_range(100..=200);
            tick.state
                .set_with_expiry(types::SNIFF_COOLDOWN, (), i64::from(cooldown));
            tick.state.erase(types::WALK_TARGET.id());
            set_pose(tick, EntityPose::Sniffing);
            true
        },
    )
}

/// Vanilla `Emerging`.
struct Emerging;

impl Behavior for Emerging {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        const CONDITIONS: [(MemoryModuleId, MemoryStatus); 3] = [
            (types::IS_EMERGING.id(), MemoryStatus::ValuePresent),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
        ];
        &CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        EMERGE_DURATION
    }

    fn max_duration(&self) -> i32 {
        EMERGE_DURATION
    }

    fn can_still_use(&mut self, _tick: &mut BrainTick<'_>) -> bool {
        true
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        set_pose(tick, EntityPose::Emerging);
        play_sound(tick, Sound::EntityWardenEmerge, 5.0, 1.0);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if has_pose(tick, EntityPose::Emerging) {
            set_pose(tick, EntityPose::Standing);
        }
    }

    fn debug_name(&self) -> &'static str {
        "Emerging"
    }
}

/// Vanilla `Digging`: sinks into the ground and despawns.
struct Digging;

impl Behavior for Digging {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        const CONDITIONS: [(MemoryModuleId, MemoryStatus); 2] = [
            (types::ATTACK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
        ];
        &CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        DIGGING_DURATION
    }

    fn max_duration(&self) -> i32 {
        DIGGING_DURATION
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let entity = tick.mob.get_entity();
        entity.on_ground.load(std::sync::atomic::Ordering::Relaxed) || entity.is_in_liquid()
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.mob.get_entity().removal_reason.load().is_none()
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if tick
            .mob
            .get_entity()
            .on_ground
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            set_pose(tick, EntityPose::Digging);
            play_sound(tick, Sound::EntityWardenDig, 5.0, 1.0);
        } else {
            play_sound(tick, Sound::EntityWardenAgitated, 5.0, 1.0);
            self.stop(tick);
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        let entity = tick.mob.get_entity();
        if entity.removal_reason.load().is_none() {
            entity.remove();
        }
    }

    fn debug_name(&self) -> &'static str {
        "Digging"
    }
}

/// Vanilla `ForceUnmount`.
struct ForceUnmount;

impl Behavior for ForceUnmount {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.mob.get_entity().has_vehicle()
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let entity = tick.mob.get_entity();
        if let Some(vehicle) = entity.get_vehicle() {
            vehicle.get_entity().remove_passenger(entity.entity_id);
        }
    }

    fn debug_name(&self) -> &'static str {
        "ForceUnmount"
    }
}

/// Vanilla `Roar`.
struct Roar;

impl Roar {
    const TICKS_BEFORE_PLAYING_ROAR_SOUND: i64 = 25;
    const ROAR_ANGER_INCREASE: i32 = 20;
}

impl Behavior for Roar {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        const CONDITIONS: [(MemoryModuleId, MemoryStatus); 4] = [
            (types::ROAR_TARGET.id(), MemoryStatus::ValuePresent),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::ROAR_SOUND_COOLDOWN.id(), MemoryStatus::Registered),
            (types::ROAR_SOUND_DELAY.id(), MemoryStatus::Registered),
        ];
        &CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        ROAR_DURATION
    }

    fn max_duration(&self) -> i32 {
        ROAR_DURATION
    }

    fn can_still_use(&mut self, _tick: &mut BrainTick<'_>) -> bool {
        true
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        tick.state.set_with_expiry(
            types::ROAR_SOUND_DELAY,
            (),
            Self::TICKS_BEFORE_PLAYING_ROAR_SOUND,
        );
        tick.state.erase(types::WALK_TARGET.id());
        let Some(target) = tick.state.get(types::ROAR_TARGET).cloned() else {
            return;
        };
        utils::look_at_entity(tick.state, target.clone());
        set_pose(tick, EntityPose::Roaring);
        if let Some(warden) = warden(tick) {
            warden.increase_anger_at(
                tick.state,
                target.as_ref(),
                Self::ROAR_ANGER_INCREASE,
                false,
            );
        }
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        if tick.state.has_memory_value(types::ROAR_SOUND_DELAY.id())
            || tick.state.has_memory_value(types::ROAR_SOUND_COOLDOWN.id())
        {
            return;
        }
        tick.state.set_with_expiry(
            types::ROAR_SOUND_COOLDOWN,
            (),
            i64::from(ROAR_DURATION) - Self::TICKS_BEFORE_PLAYING_ROAR_SOUND,
        );
        play_sound(tick, Sound::EntityWardenRoar, 3.0, 1.0);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if has_pose(tick, EntityPose::Roaring) {
            set_pose(tick, EntityPose::Standing);
        }
        if let Some(target) = tick.state.get(types::ROAR_TARGET).cloned() {
            WardenEntity::set_attack_target(tick.state, target);
        }
        tick.state.erase(types::ROAR_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "Roar"
    }
}

/// Vanilla `Sniffing`.
struct Sniffing;

impl Sniffing {
    const ANGER_FROM_SNIFFING_MAX_DISTANCE_XZ: f64 = 6.0;
    const ANGER_FROM_SNIFFING_MAX_DISTANCE_Y: f64 = 20.0;
}

impl Behavior for Sniffing {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        const CONDITIONS: [(MemoryModuleId, MemoryStatus); 7] = [
            (types::IS_SNIFFING.id(), MemoryStatus::ValuePresent),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::NEAREST_ATTACKABLE.id(), MemoryStatus::Registered),
            (types::DISTURBANCE_LOCATION.id(), MemoryStatus::Registered),
            (types::SNIFF_COOLDOWN.id(), MemoryStatus::Registered),
        ];
        &CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        SNIFFING_DURATION
    }

    fn max_duration(&self) -> i32 {
        SNIFFING_DURATION
    }

    fn can_still_use(&mut self, _tick: &mut BrainTick<'_>) -> bool {
        true
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        play_sound(tick, Sound::EntityWardenSniff, 5.0, 1.0);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if has_pose(tick, EntityPose::Sniffing) {
            set_pose(tick, EntityPose::Standing);
        }
        tick.state.erase(types::IS_SNIFFING.id());
        let Some(warden) = warden(tick) else {
            return;
        };
        let Some(target) = tick
            .state
            .get(types::NEAREST_ATTACKABLE)
            .cloned()
            .filter(|target| warden.can_target_entity(target.as_ref()))
        else {
            return;
        };
        if closer_than(
            tick,
            target.as_ref(),
            Self::ANGER_FROM_SNIFFING_MAX_DISTANCE_XZ,
            Self::ANGER_FROM_SNIFFING_MAX_DISTANCE_Y,
        ) {
            warden.increase_anger_at_default(tick.state, target.as_ref());
        }
        if !tick
            .state
            .has_memory_value(types::DISTURBANCE_LOCATION.id())
        {
            set_disturbance_location(warden, tick.state, target.get_entity().block_pos.load());
        }
    }

    fn debug_name(&self) -> &'static str {
        "Sniffing"
    }
}

/// Vanilla `SonicBoom`.
struct SonicBoom;

impl SonicBoom {
    const DISTANCE_XZ: f64 = 15.0;
    const DISTANCE_Y: f64 = 20.0;
    const KNOCKBACK_VERTICAL: f64 = 0.5;
    const KNOCKBACK_HORIZONTAL: f64 = 2.5;
    const TICKS_BEFORE_PLAYING_SOUND: i64 = 34;
    const DURATION: i32 = 60;
    const DAMAGE: f32 = 10.0;
}

/// Vanilla `SonicBoom.setCooldown`.
pub fn set_sonic_boom_cooldown(state: &mut BrainState, cooldown: i64) {
    state.set_with_expiry(types::SONIC_BOOM_COOLDOWN, (), cooldown);
}

impl Behavior for SonicBoom {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        const CONDITIONS: [(MemoryModuleId, MemoryStatus); 4] = [
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (types::SONIC_BOOM_COOLDOWN.id(), MemoryStatus::ValueAbsent),
            (
                types::SONIC_BOOM_SOUND_COOLDOWN.id(),
                MemoryStatus::Registered,
            ),
            (types::SONIC_BOOM_SOUND_DELAY.id(), MemoryStatus::Registered),
        ];
        &CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        Self::DURATION
    }

    fn max_duration(&self) -> i32 {
        Self::DURATION
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state
            .get(types::ATTACK_TARGET)
            .cloned()
            .is_some_and(|target| {
                closer_than(tick, target.as_ref(), Self::DISTANCE_XZ, Self::DISTANCE_Y)
            })
    }

    fn can_still_use(&mut self, _tick: &mut BrainTick<'_>) -> bool {
        true
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        tick.state
            .set_with_expiry(types::ATTACK_COOLING_DOWN, true, i64::from(Self::DURATION));
        tick.state.set_with_expiry(
            types::SONIC_BOOM_SOUND_DELAY,
            (),
            Self::TICKS_BEFORE_PLAYING_SOUND,
        );
        tick.world.send_entity_status(
            tick.mob.get_entity(),
            EntityStatus::SonicCharge,
            ActorEventID::None,
        );
        play_sound(tick, Sound::EntityWardenSonicCharge, 3.0, 1.0);
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let target = tick.state.get(types::ATTACK_TARGET).cloned();
        if let Some(target) = &target {
            tick.mob
                .get_mob_entity()
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .look_at_position(tick.mob, target.get_entity().pos.load());
        }
        if tick
            .state
            .has_memory_value(types::SONIC_BOOM_SOUND_DELAY.id())
            || tick
                .state
                .has_memory_value(types::SONIC_BOOM_SOUND_COOLDOWN.id())
        {
            return;
        }
        tick.state.set_with_expiry(
            types::SONIC_BOOM_SOUND_COOLDOWN,
            (),
            i64::from(Self::DURATION) - Self::TICKS_BEFORE_PLAYING_SOUND,
        );
        let Some(warden) = warden(tick) else {
            return;
        };
        let Some(target) = target.filter(|target| {
            warden.can_target_entity(target.as_ref())
                && closer_than(tick, target.as_ref(), Self::DISTANCE_XZ, Self::DISTANCE_Y)
        }) else {
            return;
        };
        let entity = tick.mob.get_entity();
        let from = entity.pos.load() + WardenEntity::CHEST_ATTACHMENT;
        let delta = target.get_entity().get_eye_pos() - from;
        let direction = delta.normalize();
        #[expect(clippy::cast_possible_truncation, reason = "vanilla floors the length")]
        let steps = delta.length().floor() as i32 + 7;
        for i in 1..steps {
            let pos = from + direction * f64::from(i);
            tick.world.spawn_particle(
                pos,
                Vector3::new(0.0, 0.0, 0.0),
                0.0,
                1,
                Particle::SonicBoom,
            );
        }
        play_sound(tick, Sound::EntityWardenSonicBoom, 3.0, 1.0);
        if target.damage_with_context(
            target.as_ref(),
            Self::DAMAGE,
            DamageType::SONIC_BOOM,
            None,
            Some(entity),
            Some(entity),
        ) {
            let resistance = target.get_living_entity().map_or(0.0, |living| {
                living.get_attribute_value(&Attributes::KNOCKBACK_RESISTANCE)
            });
            let vertical = Self::KNOCKBACK_VERTICAL * (1.0 - resistance);
            let horizontal = Self::KNOCKBACK_HORIZONTAL * (1.0 - resistance);
            target.get_entity().add_velocity(Vector3::new(
                direction.x * horizontal,
                direction.y * vertical,
                direction.z * horizontal,
            ));
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        set_sonic_boom_cooldown(tick.state, SONIC_BOOM_COOLDOWN);
    }

    fn debug_name(&self) -> &'static str {
        "SonicBoom"
    }
}
