//! Vanilla `Breeze` and `BreezeAi`: a trial chamber mob that slides around its target,
//! leaps behind it and fires wind charges.

use crate::entity::projectile_deflection::{ProjectileDeflection, default_deflection};
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use pumpkin_data::damage::DamageType;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::{EntityPose, EntityType};
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockState};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::random::{RandomImpl, legacy_rand::LegacyRand};
use pumpkin_util::{Difficulty, math::wrap_degrees};
use pumpkin_world::clip::{ClipBlock, ClipContext, ClipFluid};
use rand::RngExt;
use rand::seq::SliceRandom;

use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainTick,
    activity::ActivityData,
    behavior::{
        Behavior, DoNothing, LookAtTargetSink, MoveToTargetSink, Swim, Timed, long_jump,
        random_stroll, run_one, start_attacking, stop_attacking_if_target_invalid,
    },
    memory::{MemoryModuleId, MemoryStatus, PackedMemories, types, walk_target::WalkTarget},
    sensing::{SensorType, is_entity_attackable},
};
use crate::entity::ai::pathfinder::node::PathType;
use crate::entity::ai::util::default_random_pos;
use crate::entity::dismount_helper::is_block_dangerous;
use crate::entity::projectile::ThrownItemEntity;
use crate::entity::projectile::wind_charge::{WIND_CHARGE_GRAVITY, WindChargeEntity};
use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity},
};

const SPEED_MULTIPLIER_WHEN_SLIDING: f32 = 0.6;
const JUMP_CIRCLE_INNER_RADIUS: f64 = 4.0;
const JUMP_CIRCLE_MIDDLE_RADIUS: f64 = 8.0;
/// Vanilla `Breeze.JUMP_CIRCLE_DISTANCE_Y`.
const JUMP_CIRCLE_DISTANCE_Y: f64 = 10.0;
const TICKS_TO_REMEMBER_SEEN_TARGET: i32 = 100;
const FALL_DISTANCE_SOUND_TRIGGER_THRESHOLD: f32 = 3.0;
/// Vanilla `BreezeUtil.MAX_LINE_OF_SIGHT_TEST_RANGE`.
const MAX_LINE_OF_SIGHT_TEST_RANGE: f64 = 50.0;

pub struct BreezeEntity {
    pub mob_entity: MobEntity,
}

impl BreezeEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        {
            let navigator = mob_entity
                .navigator
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // Vanilla `ON_TOP_OF_TRAPDOOR` and `FIRE`.
            navigator.set_pathfinding_malus(PathType::DangerTrapdoor, -1.0);
            navigator.set_pathfinding_malus(PathType::DamageFire, -1.0);
        };
        Arc::new(Self { mob_entity })
    }

    /// Vanilla `getFiringYPosition`.
    fn firing_y_position(&self) -> f64 {
        let entity = self.get_entity();
        entity.pos.load().y + f64::from(entity.height()) / 2.0 + 0.3
    }

    /// Vanilla `withinInnerCircleRange`.
    fn within_inner_circle_range(&self, pos: Vector3<f64>) -> bool {
        let center = self.get_entity().block_pos.load().to_centered_f64();
        let (dx, dz) = (pos.x - center.x, pos.z - center.z);
        dx * dx + dz * dz < JUMP_CIRCLE_INNER_RADIUS * JUMP_CIRCLE_INNER_RADIUS
            && (pos.y - center.y).abs() < JUMP_CIRCLE_DISTANCE_Y
    }

    /// Vanilla `Entity.lookAt(EntityAnchorArgument.Anchor.EYES, pos)`: turns at once.
    fn look_at_from_eyes(&self, target: Vector3<f64>) {
        let entity = self.get_entity();
        let delta = target - entity.get_eye_pos();
        let pitch = wrap_degrees(-(delta.y.atan2(delta.x.hypot(delta.z)) as f32).to_degrees());
        let yaw = wrap_degrees((delta.z.atan2(delta.x) as f32).to_degrees() - 90.0);
        entity.set_rotation(yaw, pitch);
        entity.head_yaw.store(yaw);
    }

    fn is_standing(&self) -> bool {
        self.get_entity().pose.load() == EntityPose::Standing
    }
}

fn breeze<'a>(tick: &BrainTick<'a>) -> Option<&'a BreezeEntity> {
    tick.mob.cast_any().downcast_ref::<BreezeEntity>()
}

/// Vanilla `BreezeUtil.randomPointBehindTarget`.
fn random_point_behind_target(target: &dyn EntityBase) -> Vector3<f64> {
    let entity = target.get_entity();
    let mut rng = rand::rng();
    let gaussian = LegacyRand::from_seed(rng.random()).next_gaussian() as f32;
    let yaw = entity.head_yaw.load() + 180.0 + gaussian * 90.0 / 2.0;
    let radius = f64::from(rng.random::<f32>().mul_add(8.0 - 4.0, 4.0));
    let yaw_rad = f64::from(yaw).to_radians();
    entity.pos.load() + Vector3::new(-yaw_rad.sin(), 0.0, yaw_rad.cos()) * radius
}

/// Vanilla `BreezeUtil.hasLineOfSight`.
fn has_line_of_sight(breeze: &BreezeEntity, target: Vector3<f64>) -> bool {
    let entity = breeze.get_entity();
    let from = entity.pos.load();
    let range = MAX_LINE_OF_SIGHT_TEST_RANGE.max(
        breeze
            .mob_entity
            .living_entity
            .get_attribute_value(&pumpkin_data::attributes::Attributes::FOLLOW_RANGE),
    );
    if from.squared_distance_to_vec(&target) > range * range {
        return false;
    }
    entity
        .world
        .load()
        .clip(&ClipContext::new(
            from,
            target,
            ClipBlock::Collider,
            ClipFluid::None,
        ))
        .is_miss
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    // Vanilla `Sensor.wasEntityAttackableLastNTicks`, a `rememberPositives` counter.
    let mut positives_left = 0;
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(Swim::new(0.8))),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
            ],
        ),
        ActivityData::with_priority_start(
            Activity::Idle,
            0,
            vec![
                Box::new(start_attacking::create(
                    |_| true,
                    |tick| tick.state.get(types::NEAREST_ATTACKABLE).cloned(),
                )),
                Box::new(start_attacking::create(
                    |_| true,
                    |tick| {
                        tick.state
                            .get(types::HURT_BY)
                            .and_then(|hurt| hurt.attacker.clone())
                            .filter(|attacker| attacker.get_living_entity().is_some())
                    },
                )),
                Box::new(Timed::new(SlideToTargetSink(MoveToTargetSink::new(20, 40)))),
                Box::new(run_one(vec![
                    (Box::new(DoNothing::new(20, 100)), 1),
                    (
                        Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_SLIDING)),
                        2,
                    ),
                ])),
            ],
        ),
        ActivityData::with_priority_start_and_conditions(
            Activity::Fight,
            0,
            vec![
                Box::new(stop_attacking_if_target_invalid::create_with(
                    move |tick, target| {
                        if is_entity_attackable(&tick.visibility(), target.as_ref()) {
                            positives_left = TICKS_TO_REMEMBER_SEEN_TARGET;
                            false
                        } else {
                            positives_left -= 1;
                            positives_left < 0
                        }
                    },
                    |_, _| {},
                    true,
                )),
                Box::new(Timed::new(Shoot)),
                Box::new(Timed::new(LongJump)),
                Box::new(Timed::new(ShootWhenStuck)),
                Box::new(Timed::new(Slide)),
            ],
            vec![
                (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
                (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            ],
        ),
    ]
}

/// Vanilla `BreezeAi.SlideToTargetSink`: walking is sliding, and it fires on arrival.
struct SlideToTargetSink(MoveToTargetSink);

impl Behavior for SlideToTargetSink {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        self.0.entry_conditions()
    }

    fn min_duration(&self) -> i32 {
        self.0.min_duration()
    }

    fn max_duration(&self) -> i32 {
        self.0.max_duration()
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        self.0.check_extra_start_conditions(tick)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        self.0.start(tick);
        tick.mob.make_sound(Sound::EntityBreezeSlide);
        tick.mob.get_entity().set_pose(EntityPose::Sliding);
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        self.0.tick(tick);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        self.0.stop(tick);
        tick.mob.get_entity().set_pose(EntityPose::Standing);
        if tick.state.has_memory_value(types::ATTACK_TARGET.id()) {
            tick.state.set_with_expiry(types::BREEZE_SHOOT, (), 60);
        }
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        self.0.can_still_use(tick)
    }

    fn debug_name(&self) -> &'static str {
        "SlideToTargetSink"
    }
}

const SHOOT_CONDITIONS: [(MemoryModuleId, MemoryStatus); 7] = [
    (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
    (types::BREEZE_SHOOT_COOLDOWN.id(), MemoryStatus::ValueAbsent),
    (types::BREEZE_SHOOT_CHARGING.id(), MemoryStatus::ValueAbsent),
    (
        types::BREEZE_SHOOT_RECOVERING.id(),
        MemoryStatus::ValueAbsent,
    ),
    (types::BREEZE_SHOOT.id(), MemoryStatus::ValuePresent),
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::BREEZE_JUMP_TARGET.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `Shoot`: inhales, then fires a wind charge at the target.
struct Shoot;

impl Shoot {
    const ATTACK_RANGE_MAX_SQRT: f64 = 256.0;
    const UNCERTAINTY_BASE: i32 = 5;
    const UNCERTAINTY_MULTIPLIER: i32 = 4;
    const PROJECTILE_MOVEMENT_SCALE: f64 = 0.7;
    const SHOOT_INITIAL_DELAY_TICKS: i64 = 15;
    const SHOOT_RECOVER_DELAY_TICKS: i64 = 4;
    const SHOOT_COOLDOWN_TICKS: i64 = 10;
}

impl Behavior for Shoot {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &SHOOT_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        (Self::SHOOT_INITIAL_DELAY_TICKS + 1 + Self::SHOOT_RECOVER_DELAY_TICKS) as i32
    }

    fn max_duration(&self) -> i32 {
        self.min_duration()
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let Some(breeze) = breeze(tick) else {
            return false;
        };
        if !breeze.is_standing() {
            return false;
        }
        let in_range = tick.state.get(types::ATTACK_TARGET).is_some_and(|target| {
            breeze
                .get_entity()
                .pos
                .load()
                .squared_distance_to_vec(&target.get_entity().pos.load())
                < Self::ATTACK_RANGE_MAX_SQRT
        });
        if !in_range {
            tick.state.erase(types::BREEZE_SHOOT.id());
        }
        in_range
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::ATTACK_TARGET.id())
            && tick.state.has_memory_value(types::BREEZE_SHOOT.id())
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if tick.state.has_memory_value(types::ATTACK_TARGET.id()) {
            tick.mob.get_entity().set_pose(EntityPose::Shooting);
        }
        tick.state.set_with_expiry(
            types::BREEZE_SHOOT_CHARGING,
            (),
            Self::SHOOT_INITIAL_DELAY_TICKS,
        );
        tick.mob
            .play_entity_sound(Sound::EntityBreezeInhale, 1.0, 1.0);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        let entity = tick.mob.get_entity();
        if entity.pose.load() == EntityPose::Shooting {
            entity.set_pose(EntityPose::Standing);
        }
        tick.state
            .set_with_expiry(types::BREEZE_SHOOT_COOLDOWN, (), Self::SHOOT_COOLDOWN_TICKS);
        tick.state.erase(types::BREEZE_SHOOT.id());
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(breeze) = breeze(tick) else {
            return;
        };
        let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
            return;
        };
        let target_entity = target.get_entity();
        breeze.look_at_from_eyes(target_entity.pos.load());
        if tick
            .state
            .has_memory_value(types::BREEZE_SHOOT_CHARGING.id())
            || tick
                .state
                .has_memory_value(types::BREEZE_SHOOT_RECOVERING.id())
        {
            return;
        }
        tick.state.set_with_expiry(
            types::BREEZE_SHOOT_RECOVERING,
            (),
            Self::SHOOT_RECOVER_DELAY_TICKS,
        );
        let entity = breeze.get_entity();
        let pos = entity.pos.load();
        let firing_y = breeze.firing_y_position();
        let target_pos = target_entity.pos.load();
        let target_y_scale = if target_entity.has_vehicle() {
            0.8
        } else {
            0.3
        };
        let dx = target_pos.x - pos.x;
        let dy = target_pos.y + f64::from(target_entity.height()) * target_y_scale - firing_y;
        let dz = target_pos.z - pos.z;

        let world = entity.world.load_full();
        let difficulty = match world.level_info.load().difficulty {
            Difficulty::Peaceful => 0,
            Difficulty::Easy => 1,
            Difficulty::Normal => 2,
            Difficulty::Hard => 3,
        };
        let spawn_pos = Vector3::new(pos.x, firing_y, pos.z);
        let charge = ThrownItemEntity::new(
            Entity::new(world.clone(), spawn_pos, &EntityType::BREEZE_WIND_CHARGE),
            entity,
            WIND_CHARGE_GRAVITY,
        );
        charge.entity.pos.store(spawn_pos);
        charge.set_velocity(
            dx,
            dy,
            dz,
            Self::PROJECTILE_MOVEMENT_SCALE,
            f64::from(Self::UNCERTAINTY_BASE - difficulty * Self::UNCERTAINTY_MULTIPLIER),
        );
        world.spawn_entity(Arc::new(WindChargeEntity::new_breeze(charge)));
        breeze.play_entity_sound(Sound::EntityBreezeShoot, 1.5, 1.0);
    }

    fn debug_name(&self) -> &'static str {
        "Shoot"
    }
}

const SHOOT_WHEN_STUCK_CONDITIONS: [(MemoryModuleId, MemoryStatus); 5] = [
    (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
    (types::BREEZE_JUMP_INHALING.id(), MemoryStatus::ValueAbsent),
    (types::BREEZE_JUMP_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::BREEZE_SHOOT.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `ShootWhenStuck`: riding, swimming or levitating, it can only shoot.
struct ShootWhenStuck;

impl Behavior for ShootWhenStuck {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &SHOOT_WHEN_STUCK_CONDITIONS
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let entity = tick.mob.get_entity();
        entity.has_vehicle()
            || entity.is_in_water()
            || tick
                .mob
                .get_mob_entity()
                .living_entity
                .has_effect(&StatusEffect::LEVITATION)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        tick.state.set_with_expiry(types::BREEZE_SHOOT, (), 60);
    }

    fn debug_name(&self) -> &'static str {
        "ShootWhenStuck"
    }
}

const SLIDE_CONDITIONS: [(MemoryModuleId, MemoryStatus); 4] = [
    (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::BREEZE_JUMP_COOLDOWN.id(), MemoryStatus::ValueAbsent),
    (types::BREEZE_SHOOT.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `Slide`: picks a spot around the target to slide to.
struct Slide;

impl Slide {
    /// Vanilla `randomPointInMiddleCircle`.
    fn random_point_in_middle_circle(
        breeze: &BreezeEntity,
        target: &dyn EntityBase,
    ) -> Vector3<f64> {
        let pos = breeze.get_entity().pos.load();
        let to_target = target.get_entity().pos.load() - pos;
        let lerp = breeze.get_random().random::<f64>();
        let distance = to_target.length()
            - lerp.mul_add(
                JUMP_CIRCLE_INNER_RADIUS - JUMP_CIRCLE_MIDDLE_RADIUS,
                JUMP_CIRCLE_MIDDLE_RADIUS,
            );
        to_target.normalize() * distance + pos
    }
}

impl Behavior for Slide {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &SLIDE_CONDITIONS
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let entity = tick.mob.get_entity();
        entity.on_ground.load(Relaxed)
            && !entity.is_in_water()
            && entity.pose.load() == EntityPose::Standing
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let Some(breeze) = breeze(tick) else {
            return;
        };
        let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
            return;
        };
        let target_pos = target.get_entity().pos.load();
        let breeze_pos = breeze.get_entity().pos.load();
        let destination = if breeze.within_inner_circle_range(target_pos) {
            default_random_pos::get_pos_away(breeze, 5, 5, target_pos).filter(|pos| {
                has_line_of_sight(breeze, *pos)
                    && target_pos.squared_distance_to_vec(pos)
                        > target_pos.squared_distance_to_vec(&breeze_pos)
            })
        } else {
            None
        };
        let destination = destination.unwrap_or_else(|| {
            if breeze.get_random().random::<bool>() {
                random_point_behind_target(target.as_ref())
            } else {
                Self::random_point_in_middle_circle(breeze, target.as_ref())
            }
        });
        tick.state.set(
            types::WALK_TARGET,
            WalkTarget::from_block_pos(
                destination.to_block_pos(),
                SPEED_MULTIPLIER_WHEN_SLIDING,
                1,
            ),
        );
    }

    fn debug_name(&self) -> &'static str {
        "Slide"
    }
}

const LONG_JUMP_CONDITIONS: [(MemoryModuleId, MemoryStatus); 7] = [
    (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
    (types::BREEZE_JUMP_COOLDOWN.id(), MemoryStatus::ValueAbsent),
    (types::BREEZE_JUMP_INHALING.id(), MemoryStatus::Registered),
    (types::BREEZE_JUMP_TARGET.id(), MemoryStatus::Registered),
    (types::BREEZE_SHOOT.id(), MemoryStatus::ValueAbsent),
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::BREEZE_LEAVING_WATER.id(), MemoryStatus::Registered),
];

/// Vanilla breeze `LongJump`: inhales, then leaps to a spot behind the target.
struct LongJump;

impl LongJump {
    const REQUIRED_AIR_BLOCKS_ABOVE: i32 = 4;
    const JUMP_COOLDOWN_TICKS: i64 = 10;
    const JUMP_COOLDOWN_WHEN_HURT_TICKS: i64 = 2;
    const INHALING_DURATION_TICKS: i64 = 10;
    const MAX_JUMP_VELOCITY_MULTIPLIER: f32 = 0.058_333_334;
    const ALLOWED_ANGLES: [i32; 5] = [40, 55, 60, 75, 80];

    /// Vanilla `canRun`.
    fn can_run(tick: &mut BrainTick<'_>, breeze: &BreezeEntity) -> bool {
        let entity = breeze.get_entity();
        if !entity.on_ground.load(Relaxed) && !entity.is_in_water() {
            return false;
        }
        // Vanilla `isInFluidDeeperThan(getFluidJumpThreshold(), ENTITY_FLOATABLE)`, water only.
        if entity.water_height.load() > entity.get_eye_height()
            || entity.touching_lava.load(Relaxed)
        {
            return false;
        }
        if tick.state.has_memory_value(types::BREEZE_JUMP_TARGET.id()) {
            return true;
        }
        let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
            return false;
        };
        let target_pos = target.get_entity().pos.load();
        let pos = entity.pos.load();
        let follow_range = breeze
            .mob_entity
            .living_entity
            .get_attribute_value(&pumpkin_data::attributes::Attributes::FOLLOW_RANGE);
        if target_pos.squared_distance_to_vec(&pos) >= follow_range * follow_range {
            tick.state.erase(types::ATTACK_TARGET.id());
            return false;
        }
        if target_pos.squared_distance_to_vec(&pos).sqrt() - JUMP_CIRCLE_INNER_RADIUS <= 0.0 {
            return false;
        }
        if !Self::can_jump_from_current_position(breeze) {
            return false;
        }
        let Some(jump_to) =
            Self::snap_to_surface(target.as_ref(), random_point_behind_target(target.as_ref()))
        else {
            return false;
        };
        let world = entity.world.load();
        if is_block_dangerous(entity.entity_type, world.get_block_state(&jump_to.down())) {
            return false;
        }
        if !has_line_of_sight(breeze, jump_to.to_centered_f64())
            && !has_line_of_sight(breeze, jump_to.up_height(4).to_centered_f64())
        {
            return false;
        }
        tick.state.set(types::BREEZE_JUMP_TARGET, jump_to);
        true
    }

    /// Vanilla `canJumpFromCurrentPosition`: not on honey, with air or water above.
    fn can_jump_from_current_position(breeze: &BreezeEntity) -> bool {
        let entity = breeze.get_entity();
        let world = entity.world.load();
        let pos = entity.block_pos.load();
        if world.get_block(&pos) == &Block::HONEY_BLOCK {
            return false;
        }
        (1..=Self::REQUIRED_AIR_BLOCKS_ABOVE).all(|height| {
            let above = pos.up_height(height);
            world.get_block_state(&above).is_air()
                || world
                    .get_fluid(&above)
                    .has_tag(&pumpkin_data::tag::Fluid::MINECRAFT_WATER)
        })
    }

    /// Vanilla `snapToSurface`: the ground within 10 blocks below the point, else above it.
    fn snap_to_surface(target: &dyn EntityBase, point: Vector3<f64>) -> Option<BlockPos> {
        let world = target.get_entity().world.load();
        for direction in [-10.0, 10.0] {
            let hit = world.clip(&ClipContext::new(
                point,
                point + Vector3::new(0.0, direction, 0.0),
                ClipBlock::Collider,
                ClipFluid::None,
            ));
            if !hit.is_miss {
                return Some(hit.location.to_block_pos().up());
            }
        }
        None
    }

    /// Vanilla `calculateOptimalJumpVector`.
    fn calculate_optimal_jump_vector(
        breeze: &BreezeEntity,
        target: Vector3<f64>,
    ) -> Option<Vector3<f64>> {
        let mut angles = Self::ALLOWED_ANGLES;
        angles.shuffle(&mut breeze.get_random());
        let living = &breeze.mob_entity.living_entity;
        let max_velocity = Self::MAX_JUMP_VELOCITY_MULTIPLIER
            * living.get_attribute_value(&pumpkin_data::attributes::Attributes::FOLLOW_RANGE)
                as f32;
        angles.into_iter().find_map(|angle| {
            let jump = long_jump::calculate_jump_vector_for_angle(
                breeze,
                target,
                max_velocity,
                angle,
                false,
            )?;
            Some(if living.has_effect(&StatusEffect::JUMP_BOOST) {
                jump + Vector3::new(
                    0.0,
                    jump.normalize().y * f64::from(living.get_jump_boost_power()),
                    0.0,
                )
            } else {
                jump
            })
        })
    }

    fn is_finished_inhaling(tick: &BrainTick<'_>) -> bool {
        !tick
            .state
            .has_memory_value(types::BREEZE_JUMP_INHALING.id())
            && tick.mob.get_entity().pose.load() == EntityPose::Inhaling
    }

    fn is_finished_jumping(tick: &BrainTick<'_>) -> bool {
        let entity = tick.mob.get_entity();
        entity.pose.load() == EntityPose::LongJumping
            && (entity.on_ground.load(Relaxed)
                || (entity.is_in_water()
                    && !tick
                        .state
                        .has_memory_value(types::BREEZE_LEAVING_WATER.id())))
    }
}

impl Behavior for LongJump {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &LONG_JUMP_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        200
    }

    fn max_duration(&self) -> i32 {
        200
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        breeze(tick).is_some_and(|breeze| Self::can_run(tick, breeze))
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.mob.get_entity().pose.load() != EntityPose::Standing
            && !tick
                .state
                .has_memory_value(types::BREEZE_JUMP_COOLDOWN.id())
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let Some(breeze) = breeze(tick) else {
            return;
        };
        if !tick
            .state
            .has_memory_value(types::BREEZE_JUMP_INHALING.id())
        {
            tick.state.set_with_expiry(
                types::BREEZE_JUMP_INHALING,
                (),
                Self::INHALING_DURATION_TICKS,
            );
        }
        let entity = breeze.get_entity();
        entity.set_pose(EntityPose::Inhaling);
        tick.world.play_sound_fine(
            Sound::EntityBreezeCharge,
            SoundCategory::Hostile,
            &entity.pos.load(),
            1.0,
            1.0,
        );
        if let Some(jump_to) = tick.state.get(types::BREEZE_JUMP_TARGET).copied() {
            breeze.look_at_from_eyes(jump_to.to_centered_f64());
        }
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(breeze) = breeze(tick) else {
            return;
        };
        let entity = breeze.get_entity();
        let in_water = entity.is_in_water();
        if !in_water
            && tick
                .state
                .has_memory_value(types::BREEZE_LEAVING_WATER.id())
        {
            tick.state.erase(types::BREEZE_LEAVING_WATER.id());
        }
        if Self::is_finished_inhaling(tick) {
            let Some(jump) = tick
                .state
                .get(types::BREEZE_JUMP_TARGET)
                .copied()
                .and_then(|to| {
                    Self::calculate_optimal_jump_vector(
                        breeze,
                        Vector3::new(
                            f64::from(to.0.x) + 0.5,
                            f64::from(to.0.y),
                            f64::from(to.0.z) + 0.5,
                        ),
                    )
                })
            else {
                entity.set_pose(EntityPose::Standing);
                return;
            };
            if in_water {
                tick.state.set(types::BREEZE_LEAVING_WATER, ());
            }
            breeze.play_entity_sound(Sound::EntityBreezeJump, 1.0, 1.0);
            entity.set_pose(EntityPose::LongJumping);
            entity.set_rotation(entity.body_yaw.load(), entity.pitch.load());
            breeze
                .mob_entity
                .living_entity
                .discard_friction
                .store(true, Relaxed);
            entity.set_velocity(jump);
        } else if Self::is_finished_jumping(tick) {
            breeze.play_entity_sound(Sound::EntityBreezeLand, 1.0, 1.0);
            entity.set_pose(EntityPose::Standing);
            breeze
                .mob_entity
                .living_entity
                .discard_friction
                .store(false, Relaxed);
            let cooldown = if tick.state.has_memory_value(types::HURT_BY.id()) {
                Self::JUMP_COOLDOWN_WHEN_HURT_TICKS
            } else {
                Self::JUMP_COOLDOWN_TICKS
            };
            tick.state
                .set_with_expiry(types::BREEZE_JUMP_COOLDOWN, (), cooldown);
            tick.state.set_with_expiry(types::BREEZE_SHOOT, (), 100);
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        let entity = tick.mob.get_entity();
        if matches!(
            entity.pose.load(),
            EntityPose::LongJumping | EntityPose::Inhaling
        ) {
            entity.set_pose(EntityPose::Standing);
        }
        tick.state.erase(types::BREEZE_JUMP_TARGET.id());
        tick.state.erase(types::BREEZE_JUMP_INHALING.id());
        tick.state.erase(types::BREEZE_LEAVING_WATER.id());
    }

    fn debug_name(&self) -> &'static str {
        "LongJump"
    }
}

impl Mob for BreezeEntity {
    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        BrainProvider {
            memory_types: &[],
            sensor_types: &[
                SensorType::NearestLivingEntities,
                SensorType::HurtBy,
                SensorType::NearestPlayers,
                SensorType::BreezeAttackEntity,
            ],
            activities,
        }
        .make_brain(self, packed)
    }

    /// Vanilla `BreezeAi.updateActivity`.
    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        tick.state
            .set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        // Vanilla `getTarget` reads the brain's attack target.
        let target = self
            .mob_entity
            .with_brain(self, |brain| brain.get(types::ATTACK_TARGET).cloned());
        self.mob_entity.set_target(target);
    }

    /// Vanilla `Breeze.canAttack`: players and iron golems only.
    fn can_attack(&self, target: &dyn EntityBase) -> bool {
        let entity_type = target.get_entity().entity_type;
        (entity_type == &EntityType::PLAYER || entity_type == &EntityType::IRON_GOLEM)
            && self.can_attack_base(target)
    }

    /// Vanilla `Breeze.isInvulnerableTo`: breezes can't hurt each other.
    fn pre_damage_with_context(
        &self,
        damage_type: DamageType,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        if cause.is_some_and(|cause| cause.get_entity().entity_type == &EntityType::BREEZE) {
            return false;
        }
        self.pre_damage(damage_type, source)
    }

    /// Vanilla `playAmbientSound` and the whirl sound play client side only.
    fn play_ambient_sound(&self) {}

    /// Vanilla `getMovementEmission` is `EVENTS`: no step sounds.
    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {}

    /// Vanilla `getMaxHeadYRot`.
    fn get_max_head_rotation(&self) -> f32 {
        30.0
    }

    /// Vanilla `getHeadRotSpeed`.
    fn get_max_look_yaw_change(&self) -> f32 {
        25.0
    }

    fn cause_fall_damage(
        &self,
        caller: &dyn EntityBase,
        fall_distance: f32,
        damage_modifier: f32,
    ) -> bool {
        if fall_distance > FALL_DISTANCE_SOUND_TRIGGER_THRESHOLD {
            caller.play_entity_sound(Sound::EntityBreezeLand, 1.0, 1.0);
        }
        self.mob_entity
            .living_entity
            .cause_fall_damage(caller, fall_distance, damage_modifier)
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn deflection(&self, projectile: &dyn EntityBase) -> ProjectileDeflection {
        let projectile_type = projectile.get_entity().entity_type;
        if projectile_type == &EntityType::WIND_CHARGE
            || projectile_type == &EntityType::BREEZE_WIND_CHARGE
        {
            ProjectileDeflection::None
        } else if default_deflection(&self.mob_entity.living_entity.entity)
            == ProjectileDeflection::Reverse
        {
            ProjectileDeflection::Breeze
        } else {
            ProjectileDeflection::None
        }
    }
}
