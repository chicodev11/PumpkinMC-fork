//! Vanilla `Fox` inner goals.

use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Weak};

use pumpkin_data::Block;
use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::math::{rot_lerp, signum};
use rand::RngExt;

use super::{FoxEntity, as_fox};
use crate::entity::EntityBase;
use crate::entity::ai::goal::melee_attack::MeleeAttackGoal;
use crate::entity::ai::goal::track_target::TrackTargetGoal;
use crate::entity::ai::goal::{Controls, Goal, GoalReach, to_goal_ticks};
use crate::entity::ai::pathfinder::NavigatorGoal;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::mob::Mob;
use crate::entity::mob::neutral::find_by_uuid;
use crate::plugin::api::events::entity::entity_target::TargetReason;

type FoxCheck = fn(&FoxEntity) -> bool;

/// A shared goal with the checks and start step of vanilla's fox subclass.
pub struct FoxGoal<G> {
    inner: G,
    start_if: Option<FoxCheck>,
    continue_if: Option<FoxCheck>,
    on_start: Option<fn(&FoxEntity)>,
}

impl<G: Goal> FoxGoal<G> {
    pub const fn new(inner: G) -> Self {
        Self {
            inner,
            start_if: None,
            continue_if: None,
            on_start: None,
        }
    }

    /// Extra `canUse` check.
    pub const fn start_if(mut self, check: FoxCheck) -> Self {
        self.start_if = Some(check);
        self
    }

    /// Extra `canUse` and `canContinueToUse` check.
    pub const fn gated(mut self, check: FoxCheck) -> Self {
        self.start_if = Some(check);
        self.continue_if = Some(check);
        self
    }

    pub const fn on_start(mut self, step: fn(&FoxEntity)) -> Self {
        self.on_start = Some(step);
        self
    }

    fn passes(mob: &dyn Mob, check: Option<FoxCheck>) -> bool {
        check.is_none_or(|check| as_fox(mob).is_some_and(check))
    }
}

impl<G: Goal> Goal for FoxGoal<G> {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        Self::passes(mob, self.start_if) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        Self::passes(mob, self.continue_if) && self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(step) = self.on_start
            && let Some(fox) = as_fox(mob)
        {
            step(fox);
        }
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.inner.should_run_every_tick()
    }

    fn can_stop(&self) -> bool {
        self.inner.can_stop()
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }

    /// The fox checks only read the fox itself.
    fn tick_reach(&self, mob: &dyn Mob, running: bool) -> Option<GoalReach> {
        self.inner.tick_reach(mob, running)
    }
}

/// Vanilla `FoxMeleeAttackGoal`: bites without a swing, never from a sitting or crouched pose.
pub fn melee_attack(speed: f64) -> FoxGoal<MeleeAttackGoal> {
    FoxGoal::new(MeleeAttackGoal::new(speed, true).performing(bite))
        .start_if(|fox| {
            !fox.is_sitting() && !fox.is_sleeping() && !fox.is_crouching() && !fox.is_faceplanted()
        })
        .on_start(|fox| fox.set_interested(false))
}

fn bite(mob: &dyn Mob, target: &dyn EntityBase) {
    if mob.get_mob_entity().try_attack(mob.get_entity(), target) {
        mob.on_attack(target);
    }
    mob.get_entity().play_sound(Sound::EntityFoxBite);
}

fn look_at(mob: &dyn Mob, target: &Arc<dyn EntityBase>, max_yaw: f32, max_pitch: f32) {
    mob.get_mob_entity()
        .look_control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .look_at_entity_with_range(target, max_yaw, max_pitch);
}

fn stop_navigation(mob: &dyn Mob) {
    mob.get_mob_entity().steered().navigator().stop();
}

fn living_target(mob: &dyn Mob) -> Option<Arc<dyn EntityBase>> {
    mob.get_mob_entity()
        .get_target()
        .filter(|target| target.get_entity().is_alive())
}

/// Vanilla `Fox.isPathClear`: room to leap at six points along the way.
fn is_path_clear(mob: &dyn Mob, target: &dyn EntityBase) -> bool {
    let entity = mob.get_entity();
    let fox = entity.pos.load();
    let target = target.get_entity().pos.load();
    let zdiff = target.z - fox.z;
    let xdiff = target.x - fox.x;
    let slope = zdiff / xdiff;
    let world = entity.world.load();
    (0..6u8).all(|i| {
        let step = f64::from(f32::from(i) / 6.0);
        let z = if slope == 0.0 { 0.0 } else { zdiff * step };
        let x = if slope == 0.0 {
            xdiff * step
        } else {
            z / slope
        };
        (1..4u8).all(|j| {
            let pos = BlockPos::floored(fox.x + x, fox.y + f64::from(j), fox.z + z);
            world.get_block_state(&pos).replaceable()
        })
    })
}

/// Vanilla `DefendTrustedTargetGoal`: attacks whatever last hurt the first trusted entity found.
pub struct DefendTrustedTargetGoal {
    track_target_goal: TrackTargetGoal,
    target_predicate: TargetPredicate,
    trusted: Option<Weak<dyn EntityBase>>,
    attacker: Option<Weak<dyn EntityBase>>,
    timestamp: i32,
}

impl DefendTrustedTargetGoal {
    #[must_use]
    pub fn new() -> Self {
        Self {
            track_target_goal: TrackTargetGoal::with_default(false),
            target_predicate: TargetPredicate::create_attackable(),
            trusted: None,
            attacker: None,
            timestamp: 0,
        }
    }

    /// Vanilla `TRUSTED_TARGET_SELECTOR` and the untrusted check.
    fn is_valid_attacker(fox: &FoxEntity, attacker: &dyn EntityBase) -> bool {
        attacker.get_living_entity().is_some_and(|living| {
            living.last_attacking_id.load(Relaxed) != 0
                && living.last_attack_time.load(Relaxed)
                    < living.entity.tick_count.load(Relaxed) + 600
        }) && !fox.trusts(attacker.get_entity().entity_uuid)
    }
}

impl Goal for DefendTrustedTargetGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if mob.get_random().random_range(0..to_goal_ticks(10)) != 0 {
            return false;
        }
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        let world = mob.get_entity().world.load();
        let Some(trusted) = fox.trusted.load().into_iter().flatten().find_map(|uuid| {
            find_by_uuid(&world, uuid).filter(|entity| entity.get_living_entity().is_some())
        }) else {
            return false;
        };
        let Some(living) = trusted.get_living_entity() else {
            return false;
        };
        let timestamp = living.get_last_hurt_by_mob_timestamp();
        let attacker = living.get_last_hurt_by_mob();
        self.trusted = Some(Arc::downgrade(&trusted));
        self.attacker = attacker.as_ref().map(Arc::downgrade);
        if timestamp == self.timestamp {
            return false;
        }
        let Some(attacker) = attacker else {
            return false;
        };
        self.target_predicate.base_max_distance =
            self.track_target_goal.follow_range(mob.get_mob_entity());
        Self::is_valid_attacker(fox, attacker.as_ref())
            && self.track_target_goal.can_track(
                mob,
                Some(attacker.as_ref()),
                &self.target_predicate,
            )
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.track_target_goal.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.track_target_goal.start_targeting(
            mob,
            self.attacker.as_ref().and_then(Weak::upgrade),
            TargetReason::TargetAttackedOwner,
        );
        if let Some(trusted) = self.trusted.as_ref().and_then(Weak::upgrade)
            && let Some(living) = trusted.get_living_entity()
        {
            self.timestamp = living.get_last_hurt_by_mob_timestamp();
        }
        if let Some(fox) = as_fox(mob) {
            fox.get_entity().play_sound(Sound::EntityFoxAggro);
            fox.set_defending(true);
            fox.set_sleeping(false);
        }
        self.track_target_goal.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.trusted = None;
        self.attacker = None;
        self.track_target_goal.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.track_target_goal.controls()
    }
}

/// Vanilla `FaceplantGoal`: stuck head first in snow for 40 ticks.
#[derive(Default)]
pub struct FaceplantGoal {
    countdown: i32,
}

impl Goal for FaceplantGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        as_fox(mob).is_some_and(FoxEntity::is_faceplanted)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.can_start(mob) && self.countdown > 0
    }

    fn start(&mut self, _mob: &dyn Mob) {
        self.countdown = self.get_tick_count(40);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(fox) = as_fox(mob) {
            fox.set_faceplanted(false);
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        self.countdown -= 1;
    }

    fn controls(&self) -> Controls {
        Controls::LOOK | Controls::JUMP | Controls::MOVE
    }
}

/// Within this squared distance the fox stops and crouches.
const STALK_DISTANCE_SQ: f64 = 36.0;

/// Vanilla `StalkPreyGoal`: sneaks up on chickens and rabbits, then crouches to pounce.
#[derive(Default)]
pub struct StalkPreyGoal;

impl StalkPreyGoal {
    fn crouch(mob: &dyn Mob, fox: &FoxEntity) {
        fox.set_interested(true);
        fox.set_crouching(true);
        stop_navigation(mob);
    }
}

impl Goal for StalkPreyGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if fox.is_sleeping() {
            return false;
        }
        let Some(target) = living_target(mob) else {
            return false;
        };
        let target_entity = target.get_entity();
        let prey = target_entity.entity_type == &EntityType::CHICKEN
            || target_entity.entity_type == &EntityType::RABBIT;
        prey && mob
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&target_entity.pos.load())
            > STALK_DISTANCE_SQ
            && !fox.is_crouching()
            && !fox.is_interested()
            && !mob.get_mob_entity().living_entity.jumping.load(Relaxed)
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(fox) = as_fox(mob) {
            fox.set_sitting(false);
            fox.set_faceplanted(false);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        match mob.get_mob_entity().get_target() {
            Some(target) if is_path_clear(mob, target.as_ref()) => {
                Self::crouch(mob, fox);
                look_at(
                    mob,
                    &target,
                    mob.get_max_head_rotation(),
                    mob.get_max_look_pitch_change(),
                );
            }
            _ => {
                fox.set_interested(false);
                fox.set_crouching(false);
            }
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let (Some(fox), Some(target)) = (as_fox(mob), mob.get_mob_entity().get_target()) else {
            return;
        };
        look_at(
            mob,
            &target,
            mob.get_max_head_rotation(),
            mob.get_max_look_pitch_change(),
        );
        let pos = mob.get_entity().pos.load();
        let target_pos = target.get_entity().pos.load();
        if pos.squared_distance_to_vec(&target_pos) <= STALK_DISTANCE_SQ {
            Self::crouch(mob, fox);
        } else {
            mob.get_mob_entity()
                .steered()
                .navigator()
                .set_progress(NavigatorGoal::new(pos, target_pos, 1.5));
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

/// Vanilla `FoxPounceGoal`: the leap from a full crouch, nose first into snow when it lands there.
#[derive(Default)]
pub struct PounceGoal;

impl PounceGoal {
    /// Pitch follows the leap's arc, then levels out once it is flat.
    fn tilt(mob: &dyn Mob) {
        let entity = mob.get_entity();
        let movement = entity.velocity.load();
        let pitch = entity.pitch.load();
        if movement.y * movement.y < f64::from(0.03f32) && pitch != 0.0 {
            entity.set_pitch(rot_lerp(0.2, pitch, 0.0));
            return;
        }
        let direction = movement.horizontal_length();
        let jumping = mob.get_mob_entity().living_entity.jumping.load(Relaxed);
        let upwards_bias = if jumping && movement.y > 0.0 {
            6.5
        } else {
            1.0
        };
        let biased_y = movement.y * upwards_bias;
        let len = direction.hypot(biased_y);
        if len > f64::from(1.0e-5f32) {
            let rotation = signum(-biased_y) * (direction / len).acos() * f64::from(180.0f32)
                / f64::from(std::f32::consts::PI);
            entity.set_pitch(rotation as f32);
        }
    }
}

impl Goal for PounceGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if !fox.is_fully_crouched() {
            return false;
        }
        // Vanilla also compares the target's motion and facing direction, always equal for mobs.
        let Some(target) = living_target(mob) else {
            return false;
        };
        let clear = is_path_clear(mob, target.as_ref());
        if !clear {
            fox.set_crouching(false);
            fox.set_interested(false);
        }
        clear
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if living_target(mob).is_none() {
            return false;
        }
        let entity = mob.get_entity();
        let dy = entity.velocity.load().y;
        let landed = dy * dy < f64::from(0.05f32)
            && entity.pitch.load().abs() < 15.0
            && entity.on_ground.load(Relaxed);
        !landed && !fox.is_faceplanted()
    }

    fn can_stop(&self) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn Mob) {
        // Vanilla's `setJumping(true)` is reset by the jump control before it acts.
        if let Some(fox) = as_fox(mob) {
            fox.set_pouncing(true);
            fox.set_interested(false);
        }
        if let Some(target) = mob.get_mob_entity().get_target() {
            look_at(mob, &target, 60.0, 30.0);
            let entity = mob.get_entity();
            let offset = target.get_entity().pos.load() - entity.pos.load();
            let length = offset.length();
            let direction = if length < f64::from(1.0e-5f32) {
                Vector3::default()
            } else {
                offset / length
            };
            entity.set_velocity(
                entity.velocity.load() + Vector3::new(direction.x * 0.8, 0.9, direction.z * 0.8),
            );
        }
        stop_navigation(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(fox) = as_fox(mob) {
            fox.set_crouching(false);
            fox.crouch_amount.store(0.0);
            fox.set_interested(false);
            fox.set_pouncing(false);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        let target = mob.get_mob_entity().get_target();
        if let Some(target) = &target {
            look_at(mob, target, 60.0, 30.0);
        }
        if !fox.is_faceplanted() {
            Self::tilt(mob);
        }

        let entity = mob.get_entity();
        if let Some(target) = target {
            let offset = entity.pos.load() - target.get_entity().pos.load();
            let (x, y, z) = (offset.x as f32, offset.y as f32, offset.z as f32);
            if (x * x + y * y + z * z).sqrt() <= 2.0 {
                if mob.get_mob_entity().try_attack(entity, target.as_ref()) {
                    mob.on_attack(target.as_ref());
                }
                return;
            }
        }
        let world = entity.world.load();
        if entity.pitch.load() > 0.0
            && entity.on_ground.load(Relaxed)
            && entity.velocity.load().y as f32 != 0.0
            && world.get_block(&entity.block_pos.load()).id == Block::SNOW.id
        {
            entity.set_pitch(60.0);
            mob.set_mob_target(None, TargetReason::Unknown);
            fox.set_faceplanted(true);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::JUMP
    }
}

/// Vanilla `FoxBehaviorGoal.hasShelter`: no sky above the fox's head and a spot it likes.
fn has_shelter(fox: &FoxEntity) -> bool {
    let entity = &fox.mob_entity.living_entity.entity;
    let pos = entity.pos.load();
    let head = BlockPos::floored(pos.x, entity.bounding_box.load().max.y, pos.z);
    !entity.world.load().can_see_sky(&head) && fox.get_walk_target_value(&head) >= 0.0
}

/// Vanilla `FoxAlertableEntitiesSelector`: what keeps a fox awake and alert.
fn is_alerting(fox: &FoxEntity, candidate: &dyn EntityBase) -> bool {
    let entity_type = candidate.get_entity().entity_type;
    if entity_type == &EntityType::FOX {
        return false;
    }
    if entity_type == &EntityType::CHICKEN
        || entity_type == &EntityType::RABBIT
        || crate::entity::ai::target_match::TargetMatch::Monster.matches(entity_type)
    {
        return true;
    }
    if let Some(tamable) = candidate.get_mob().and_then(|mob| mob.as_tamable()) {
        return !tamable.is_tame();
    }
    if let Some(player) = candidate.get_player() {
        if player.is_spectator() || player.is_creative() {
            return false;
        }
        if fox.trusts(player.gameprofile.id) {
            return false;
        }
    }
    let sleeping = candidate
        .get_player()
        .is_some_and(crate::entity::player::Player::is_sleeping);
    !sleeping && !candidate.get_entity().is_sneaking()
}

/// Vanilla `FoxBehaviorGoal.alertable`: any alerting creature within 12 blocks (6 up or
/// down), seen or not.
fn alertable(fox: &FoxEntity) -> bool {
    let entity = &fox.mob_entity.living_entity.entity;
    let world = entity.world.load();
    let area = entity.bounding_box.load().expand(12.0, 6.0, 12.0);
    let targeting = TargetPredicate::create_attackable()
        .set_base_max_distance(12.0)
        .ignore_visibility();
    let mut candidates = world.get_entities_at_box(&area);
    candidates.extend(
        world
            .players
            .load()
            .iter()
            .filter(|player| area.intersects(&player.get_entity().bounding_box.load()))
            .map(|player| player.clone() as Arc<dyn EntityBase>),
    );
    candidates.iter().any(|candidate| {
        candidate.get_entity().entity_id != entity.entity_id
            && candidate.get_living_entity().is_some()
            && targeting.test(&world, Some(fox), candidate.as_ref())
            && is_alerting(fox, candidate.as_ref())
    })
}

/// Vanilla `SeekShelterGoal`: flee to cover in a thunderstorm, or out of the open daylight
/// when not in a village.
pub struct SeekShelterGoal {
    inner: crate::entity::ai::goal::flee_sun::FleeSunGoal,
    interval: i32,
}

impl SeekShelterGoal {
    #[must_use]
    pub const fn new(speed: f64) -> Self {
        Self {
            inner: crate::entity::ai::goal::flee_sun::FleeSunGoal::new(speed),
            interval: to_goal_ticks(100),
        }
    }
}

impl Goal for SeekShelterGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if fox.is_sleeping() || mob.get_mob_entity().get_target().is_some() {
            return false;
        }
        let entity = mob.get_entity();
        let world = entity.world.load();
        let pos = entity.block_pos.load();
        if super::is_thundering(&world) && world.can_see_sky(&pos) {
            return self.inner.set_wanted_pos(mob);
        }
        if self.interval > 0 {
            self.interval -= 1;
            return false;
        }
        // Vanilla resets to a raw 100, not a reduced delay, after the first wait.
        self.interval = 100;
        world.is_bright_outside()
            && world.can_see_sky(&pos)
            && !world.is_village(&pos)
            && self.inner.set_wanted_pos(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(fox) = as_fox(mob) {
            fox.clear_states();
        }
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

/// Vanilla `SleepGoal.WAIT_TIME_BEFORE_SLEEP`.
const WAIT_TIME_BEFORE_SLEEP: i32 = to_goal_ticks(140);

/// Vanilla `SleepGoal`: a still fox under cover sleeps through the day unless something
/// alerting is near.
pub struct SleepGoal {
    countdown: i32,
}

impl SleepGoal {
    #[must_use]
    pub fn new() -> Self {
        Self {
            countdown: rand::rng().random_range(0..WAIT_TIME_BEFORE_SLEEP),
        }
    }

    fn can_sleep(&mut self, fox: &FoxEntity) -> bool {
        if self.countdown > 0 {
            self.countdown -= 1;
            return false;
        }
        let entity = &fox.mob_entity.living_entity.entity;
        entity.world.load().is_bright_outside()
            && has_shelter(fox)
            && !alertable(fox)
            && !entity.is_in_powder_snow()
    }
}

impl Default for SleepGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for SleepGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        // Vanilla: only while the fox is not being steered (xxa, yya, zza all 0).
        let input = fox.mob_entity.living_entity.movement_input.load();
        if input.x != 0.0 || input.y != 0.0 || input.z != 0.0 {
            return false;
        }
        self.can_sleep(fox) || fox.is_sleeping()
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        as_fox(mob).is_some_and(|fox| self.can_sleep(fox))
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        fox.set_sitting(false);
        fox.set_crouching(false);
        fox.set_interested(false);
        fox.mob_entity.living_entity.jumping.store(false, Relaxed);
        fox.set_sleeping(true);
        fox.mob_entity.steered().navigator().stop();
        let pos = fox.mob_entity.living_entity.entity.pos.load();
        fox.mob_entity
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_wanted_position(pos.x, pos.y, pos.z, 0.0);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.countdown = mob.get_random().random_range(0..WAIT_TIME_BEFORE_SLEEP);
        if let Some(fox) = as_fox(mob) {
            fox.clear_states();
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK | Controls::JUMP
    }
}

/// Vanilla `PerchAndSearchGoal`: an idle fox sits and looks around a few times.
#[derive(Default)]
pub struct PerchAndSearchGoal {
    rel_x: f64,
    rel_z: f64,
    look_time: i32,
    looks_remaining: i32,
}

impl PerchAndSearchGoal {
    fn reset_look(&mut self, mob: &dyn Mob) {
        let angle = std::f64::consts::TAU * mob.get_random().random::<f64>();
        self.rel_x = angle.cos();
        self.rel_z = angle.sin();
        self.look_time = self.get_tick_count(80 + mob.get_random().random_range(0..20));
    }
}

impl Goal for PerchAndSearchGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        fox.mob_entity
            .living_entity
            .get_last_hurt_by_mob()
            .is_none()
            && mob.get_random().random::<f32>() < 0.02
            && !fox.is_sleeping()
            && mob.get_mob_entity().get_target().is_none()
            && mob.is_navigator_idle()
            && !alertable(fox)
            && !fox.is_pouncing()
            && !fox.is_crouching()
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        self.looks_remaining > 0
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.reset_look(mob);
        self.looks_remaining = 2 + mob.get_random().random_range(0..3);
        if let Some(fox) = as_fox(mob) {
            fox.set_sitting(true);
        }
        mob.get_mob_entity().steered().navigator().stop();
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(fox) = as_fox(mob) {
            fox.set_sitting(false);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.look_time -= 1;
        if self.look_time <= 0 {
            self.looks_remaining -= 1;
            self.reset_look(mob);
        }
        let entity = mob.get_entity();
        let pos = entity.pos.load();
        mob.get_mob_entity()
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_with_range(
                pos.x + self.rel_x,
                entity.get_eye_y(),
                pos.z + self.rel_z,
                mob.get_max_look_yaw_change(),
                mob.get_max_look_pitch_change(),
            );
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

/// Vanilla `Fox.canMove`.
fn can_move(fox: &FoxEntity) -> bool {
    !fox.is_sleeping() && !fox.is_sitting() && !fox.is_faceplanted()
}

/// Item entities a fox goes for: vanilla `ALLOWED_ITEMS` (no pickup delay, alive), within
/// 8 blocks.
fn nearby_items(fox: &FoxEntity) -> Vec<Arc<dyn EntityBase>> {
    let entity = &fox.mob_entity.living_entity.entity;
    let area = entity.bounding_box.load().expand(8.0, 8.0, 8.0);
    entity
        .world
        .load()
        .get_entities_at_box(&area)
        .into_iter()
        .filter(|candidate| {
            candidate.get_item_entity().is_some_and(|item| {
                item.get_pickup_delay() == 0 && candidate.get_entity().is_alive()
            })
        })
        .collect()
}

/// Vanilla `FoxSearchForItemsGoal`: an idle fox with an empty mouth walks to nearby items.
pub struct SearchForItemsGoal;

impl SearchForItemsGoal {
    fn go_to_first(mob: &dyn Mob, items: &[Arc<dyn EntityBase>]) {
        if let Some(item) = items.first() {
            let steered = mob.get_mob_entity().steered();
            let mut navigator = steered.navigator();
            let living = &steered.mob_entity().living_entity;
            let path = navigator.create_path(living, item.get_entity().pos.load(), 0);
            navigator.move_to_path(path, 1.2, living);
        }
    }
}

impl Goal for SearchForItemsGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if !fox.held_item().is_empty() {
            return false;
        }
        if mob.get_mob_entity().get_target().is_some()
            || fox
                .mob_entity
                .living_entity
                .get_last_hurt_by_mob()
                .is_some()
        {
            return false;
        }
        if !can_move(fox) || mob.get_random().random_range(0..to_goal_ticks(10)) != 0 {
            return false;
        }
        !nearby_items(fox).is_empty() && fox.held_item().is_empty()
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(fox) = as_fox(mob) {
            Self::go_to_first(mob, &nearby_items(fox));
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if let Some(fox) = as_fox(mob)
            && fox.held_item().is_empty()
        {
            Self::go_to_first(mob, &nearby_items(fox));
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// A block-state property value by name.
fn property_value(state: pumpkin_data::BlockStateId, key: &str) -> Option<&'static str> {
    Block::from_state_id(state)
        .properties(state)?
        .to_props()
        .into_iter()
        .find(|prop| prop.key_str() == key)
        .map(pumpkin_data::Property::value_str)
}

/// `state` with one property changed, vanilla `setValue`.
fn with_property(
    state: pumpkin_data::BlockStateId,
    key: &str,
    value: &str,
) -> pumpkin_data::BlockStateId {
    let block = Block::from_state_id(state);
    let Some(props) = block.properties(state) else {
        return state;
    };
    let props: Vec<_> = props
        .to_props()
        .into_iter()
        .map(|prop| {
            if prop.key_str() == key {
                pumpkin_data::Property::from_str_pair(key, value).unwrap_or(prop)
            } else {
                prop
            }
        })
        .collect();
    block.from_properties(&props).to_state_id(block)
}

/// Vanilla `CaveVines.hasGlowBerries`.
fn has_glow_berries(state: pumpkin_data::BlockStateId) -> bool {
    property_value(state, "berries") == Some("true")
}

/// The block test of [`EatBerriesGoal`]: ripe sweet berry bushes or berried cave vines.
pub struct BerryTarget;

impl crate::entity::ai::goal::move_to_target_pos::MoveToTargetPos for BerryTarget {
    fn is_target_pos(&self, world: &crate::world::World, pos: BlockPos) -> bool {
        let state = world.get_block_state_id(&pos);
        let ripe_bush = Block::from_state_id(state) == &Block::SWEET_BERRY_BUSH
            && property_value(state, "age")
                .and_then(|age| age.parse::<u8>().ok())
                .is_some_and(|age| age >= 2);
        ripe_bush || has_glow_berries(state)
    }

    /// Vanilla `acceptedDistance`.
    fn get_desired_distance_to_target(&self) -> f64 {
        2.0
    }

    /// Vanilla `shouldRecalculatePath`: `tryTicks % 100 == 0`.
    fn should_recalculate_path(&self, trying_time: i32) -> bool {
        trying_time % 100 == 0
    }
}

/// Vanilla `FoxEatBerriesGoal`: walks to ripe berries, waits 2 seconds, and picks them,
/// keeping one in its mouth.
pub struct EatBerriesGoal {
    // Boxed so the handle inside `inner` keeps pointing at it.
    _target: Box<BerryTarget>,
    inner: crate::entity::ai::goal::move_to_target_pos::MoveToTargetPosGoal<BerryTarget>,
    ticks_waited: i32,
}

impl EatBerriesGoal {
    /// `MoveToBlockGoal(fox, speed, range, verticalRange)`.
    #[must_use]
    pub fn new(speed: f64, range: i32, vertical_range: i32) -> Self {
        let target = Box::new(BerryTarget);
        // SAFETY: `target` is heap allocated, owned by the goal next to `inner` and never
        // replaced, so the handle stays valid for the goal's whole life.
        let handle = unsafe { crate::entity::ai::goal::ParentHandle::new(&*target) };
        Self {
            _target: target,
            inner: crate::entity::ai::goal::move_to_target_pos::MoveToTargetPosGoal::new(
                handle,
                speed,
                range,
                vertical_range,
            ),
            ticks_waited: 0,
        }
    }

    /// Vanilla `onReachedTarget`.
    fn on_reached_target(&self, fox: &FoxEntity) {
        let entity = &fox.mob_entity.living_entity.entity;
        let world = entity.world.load_full();
        if !world.level_info.load().game_rules.mob_griefing {
            return;
        }
        let pos = self.inner.target_pos;
        let state = world.get_block_state_id(&pos);
        if Block::from_state_id(state) == &Block::SWEET_BERRY_BUSH {
            Self::pick_sweet_berries(fox, &world, pos, state);
        } else if has_glow_berries(state) {
            // Vanilla `CaveVines.use`.
            world.drop_stack(
                &pos,
                pumpkin_data::item_stack::ItemStack::new(
                    1,
                    &pumpkin_data::item::Item::GLOW_BERRIES,
                ),
            );
            let pitch = 0.8 + rand::random::<f32>() * 0.4;
            world.play_sound_fine(
                Sound::BlockCaveVinesPickBerries,
                pumpkin_data::sound::SoundCategory::Blocks,
                &pos.to_centered_f64(),
                1.0,
                pitch,
            );
            world.set_block_state(
                &pos,
                with_property(state, "berries", "false"),
                pumpkin_world::world::BlockFlags::NOTIFY_LISTENERS,
            );
        }
    }

    /// Vanilla `pickSweetBerries`: 1-2 berries (one more when fully grown), the first into
    /// an empty mouth, the bush back to age 1.
    fn pick_sweet_berries(
        fox: &FoxEntity,
        world: &Arc<crate::world::World>,
        pos: BlockPos,
        state: pumpkin_data::BlockStateId,
    ) {
        let age = property_value(state, "age")
            .and_then(|age| age.parse::<u8>().ok())
            .unwrap_or(0);
        let mut count = 1 + rand::rng().random_range(0..2) + u8::from(age == 3);
        if fox.held_item().is_empty() {
            fox.mob_entity.set_item_slot(
                &pumpkin_data::data_component_impl::EquipmentSlot::MAIN_HAND,
                pumpkin_data::item_stack::ItemStack::new(
                    1,
                    &pumpkin_data::item::Item::SWEET_BERRIES,
                ),
            );
            count -= 1;
        }
        if count > 0 {
            world.drop_stack(
                &pos,
                pumpkin_data::item_stack::ItemStack::new(
                    count,
                    &pumpkin_data::item::Item::SWEET_BERRIES,
                ),
            );
        }
        fox.get_entity()
            .play_sound(Sound::BlockSweetBerryBushPickBerries);
        world.set_block_state(
            &pos,
            with_property(state, "age", "1"),
            pumpkin_world::world::BlockFlags::NOTIFY_LISTENERS,
        );
    }
}

impl Goal for EatBerriesGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        as_fox(mob).is_some_and(|fox| !fox.is_sleeping()) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.ticks_waited = 0;
        if let Some(fox) = as_fox(mob) {
            fox.set_sitting(false);
        }
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        if self.inner.is_reached_target() {
            if self.ticks_waited >= 40 {
                self.on_reached_target(fox);
            } else {
                self.ticks_waited += 1;
            }
        } else if mob.get_random().random::<f32>() < 0.05 {
            fox.get_entity().play_sound(Sound::EntityFoxSniff);
        }
        self.inner.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.inner.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

/// Vanilla `FoxStrollThroughVillageGoal` (on `StrollThroughVillageGoal`): at night, an idle
/// fox near a village heads deeper into it.
pub struct StrollThroughVillageGoal {
    interval: i32,
    wanted_pos: Option<BlockPos>,
}

impl StrollThroughVillageGoal {
    #[must_use]
    pub const fn new(interval: i32) -> Self {
        Self {
            interval: to_goal_ticks(interval),
            wanted_pos: None,
        }
    }

    /// Vanilla `canFoxMove`.
    fn can_fox_move(fox: &FoxEntity) -> bool {
        !fox.is_sleeping()
            && !fox.is_sitting()
            && !fox.is_defending()
            && fox.mob_entity.get_target().is_none()
    }

    /// `navigation.moveTo(x, y, z, 1.0)`, reporting whether a route was set.
    fn move_to(mob: &dyn Mob, pos: BlockPos) -> bool {
        let steered = mob.get_mob_entity().steered();
        let mut navigator = steered.navigator();
        let living = &steered.mob_entity().living_entity;
        let path = navigator.create_path(living, pos.to_f64(), 0);
        navigator.move_to_path(path, 1.0, living)
    }

    fn surface(world: &crate::world::World, pos: BlockPos) -> BlockPos {
        let y = world.get_heightmap_height(
            pumpkin_world::chunk::ChunkHeightmapType::MotionBlockingNoLeaves,
            pos.0.x,
            pos.0.z,
        ) + 1;
        BlockPos::new(pos.0.x, y, pos.0.z)
    }
}

impl Goal for StrollThroughVillageGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        let entity = mob.get_entity();
        let world = entity.world.load_full();
        if mob.get_controlling_passenger().is_some()
            || world.is_bright_outside()
            || mob.get_random().random_range(0..self.interval.max(1)) != 0
            || !world.is_close_to_village(&entity.block_pos.load(), 6)
        {
            return false;
        }
        // Vanilla weighs candidates by `-sectionsToVillage`: closer to the village centre.
        self.wanted_pos =
            crate::entity::ai::util::land_random_pos::get_pos_weighted(mob, 15, 7, |pos| {
                -f64::from(world.sections_to_village(pumpkin_world::poi::SectionPos::of(&pos)))
            })
            .map(BlockPos::floored_v);
        self.wanted_pos.is_some() && Self::can_fox_move(fox)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(wanted) = self.wanted_pos else {
            return false;
        };
        let steered = mob.get_mob_entity().steered();
        let navigator = steered.navigator();
        !navigator.is_done()
            && navigator.get_target_pos() == Some(wanted)
            && as_fox(mob).is_some_and(Self::can_fox_move)
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(fox) = as_fox(mob) {
            fox.clear_states();
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(wanted) = self.wanted_pos else {
            return;
        };
        let entity = mob.get_entity();
        let mob_pos = entity.pos.load();
        let wanted_center = Vector3::new(
            f64::from(wanted.0.x) + 0.5,
            f64::from(wanted.0.y),
            f64::from(wanted.0.z) + 0.5,
        );
        let idle = mob.is_navigator_idle();
        let close = wanted.dist_to_center_sqr(mob_pos.x, mob_pos.y, mob_pos.z) < 100.0;
        if !idle || close {
            return;
        }
        // Vanilla: a step 10 blocks towards a point 40% of the way back from the target.
        let pulled = (mob_pos - wanted_center) * 0.4 + wanted_center;
        let step = (pulled - mob_pos).normalize() * 10.0 + mob_pos;
        let world = entity.world.load();
        let target = Self::surface(&world, BlockPos::floored_v(step));
        if !Self::move_to(mob, target) {
            // Vanilla `moveRandomly`.
            let origin = entity.block_pos.load();
            let (dx, dz) = {
                let mut rng = mob.get_random();
                (rng.random_range(0..16) - 8, rng.random_range(0..16) - 8)
            };
            let random = Self::surface(
                &world,
                BlockPos::new(origin.0.x + dx, origin.0.y, origin.0.z + dz),
            );
            Self::move_to(mob, random);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

#[cfg(test)]
mod behavior_tests {
    use super::*;
    use crate::entity::Entity;
    use crate::entity::ai::test_support::world;

    #[tokio::test]
    async fn chickens_alert_foxes_but_other_foxes_do_not() -> Result<(), Box<dyn std::error::Error>>
    {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let fox = FoxEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::FOX,
        ));
        world.spawn_entity(fox.clone());
        let quiet = alertable(&fox);

        let other = FoxEntity::new(Entity::new(
            world.clone(),
            Vector3::new(3.5, 64.0, 0.5),
            &EntityType::FOX,
        ));
        world.spawn_entity(other);
        let with_fox = alertable(&fox);

        let chicken = crate::entity::r#type::from_type(
            &EntityType::CHICKEN,
            Vector3::new(5.5, 64.0, 0.5),
            &world,
            uuid::Uuid::new_v4(),
        );
        world.spawn_entity(chicken);
        let with_chicken = alertable(&fox);
        world.level.shutdown().await;

        assert!(!quiet && !with_fox, "foxes do not alert each other");
        assert!(
            with_chicken,
            "a chicken within 12 blocks keeps the fox alert"
        );
        Ok(())
    }

    #[tokio::test]
    async fn perching_sits_the_fox_down_and_counts_looks() -> Result<(), Box<dyn std::error::Error>>
    {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let fox = FoxEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::FOX,
        ));
        let mut goal = PerchAndSearchGoal::default();
        goal.start(&*fox);
        let sitting = fox.is_sitting();
        let looks = goal.looks_remaining;
        goal.stop(&*fox);
        let stood_up = !fox.is_sitting();
        world.level.shutdown().await;

        assert!(sitting && stood_up);
        assert!(
            (2..=4).contains(&looks),
            "2 + nextInt(3) looks, got {looks}"
        );
        Ok(())
    }

    fn spawn_fox(world: &Arc<crate::world::World>) -> Arc<FoxEntity> {
        let fox = FoxEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::FOX,
        ));
        world.spawn_entity(fox.clone());
        fox
    }

    #[tokio::test]
    async fn fox_eats_held_food_after_thirty_seconds() -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let fox = spawn_fox(&world);
        fox.get_entity().on_ground.store(true, Relaxed);
        fox.mob_entity.set_item_slot(
            &pumpkin_data::data_component_impl::EquipmentSlot::MAIN_HAND,
            pumpkin_data::item_stack::ItemStack::new(1, &pumpkin_data::item::Item::SWEET_BERRIES),
        );
        for _ in 0..600 {
            fox.mob_tick(&*fox);
        }
        let before = fox.held_item().is_empty();
        fox.mob_tick(&*fox);
        let after = fox.held_item().is_empty();
        world.level.shutdown().await;

        assert!(!before, "still chewing at 600 ticks");
        assert!(after, "eaten on tick 601");
        Ok(())
    }

    #[tokio::test]
    async fn fox_swaps_a_non_food_item_for_food_only() -> Result<(), Box<dyn std::error::Error>> {
        use pumpkin_data::item::Item;
        use pumpkin_data::item_stack::ItemStack;
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let fox = spawn_fox(&world);
        let empty_takes_anything = fox.can_hold_item(&ItemStack::new(1, &Item::STICK));
        fox.mob_entity.set_item_slot(
            &pumpkin_data::data_component_impl::EquipmentSlot::MAIN_HAND,
            ItemStack::new(1, &Item::STICK),
        );
        fox.ticks_since_eaten.store(5, Relaxed);
        let takes_food = fox.can_hold_item(&ItemStack::new(1, &Item::SWEET_BERRIES));
        let takes_stick = fox.can_hold_item(&ItemStack::new(1, &Item::FEATHER));
        world.level.shutdown().await;

        assert!(empty_takes_anything && takes_food && !takes_stick);
        Ok(())
    }

    #[tokio::test]
    async fn picking_a_ripe_bush_resets_it_and_fills_the_mouth()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        world.level.loaded_chunks.insert(
            pumpkin_util::math::vector2::Vector2::new(0, 0),
            pumpkin_world::chunk::ChunkData::empty_sync(0, 0),
        );
        let pos = BlockPos::new(2, 64, 2);
        let ripe = with_property(Block::SWEET_BERRY_BUSH.default_state.id, "age", "3");
        world.set_block_state(
            &pos,
            ripe,
            pumpkin_world::world::BlockFlags::SKIP_BLOCK_ADDED_CALLBACK,
        );
        let fox = spawn_fox(&world);
        let valid = crate::entity::ai::goal::move_to_target_pos::MoveToTargetPos::is_target_pos(
            &BerryTarget,
            &world,
            pos,
        );
        EatBerriesGoal::pick_sweet_berries(&fox, &world, pos, ripe);
        let age = property_value(world.get_block_state_id(&pos), "age");
        let held = fox.held_item().item.id;
        world.level.shutdown().await;

        assert!(valid, "an age 3 bush is a berry target");
        assert_eq!(age, Some("1"));
        assert_eq!(held, pumpkin_data::item::Item::SWEET_BERRIES.id);
        Ok(())
    }
}
