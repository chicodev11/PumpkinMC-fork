//! Vanilla `Vex`: a wall-phasing flyer that charges its target, follows its summoner's target
//! and dies after a limited life.
//!
//! Ported from the 26.3 `Vex` class and its inner goals and move control.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::damage::DamageType;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;
use uuid::Uuid;

use crate::entity::ai::behavior::hunting::{self, Hunt};
use crate::entity::ai::control::move_control::Operation;
use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::ai::goal::look_at_entity::LookAtEntityGoal;
use crate::entity::ai::goal::swim::SwimGoal;
use crate::entity::ai::goal::track_target::TrackTargetGoal;
use crate::entity::ai::goal::{Controls, Goal, to_goal_ticks};
use crate::entity::ai::target_match::TargetMatch;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::mob::raider;
use crate::entity::mob::spawn::{EntitySpawnReason, SpawnGroupData};
use crate::entity::mob::{Mob, MobEntity};
use crate::entity::{Entity, EntityBase};
use crate::plugin::api::events::entity::entity_target::TargetReason;
use crate::world::World;

const VEX_HUNTS: &[Hunt] = &[Hunt::new(3, TargetMatch::Type(&EntityType::PLAYER), true)];

/// Vanilla `Vex.TICKS_PER_FLAP`, `Mth.ceil(3.9269907F)`.
const TICKS_PER_FLAP: i32 = 4;
/// Vanilla `FLAG_IS_CHARGING`.
const FLAG_IS_CHARGING: i8 = 1;

pub struct VexEntity {
    pub mob_entity: MobEntity,
    /// Vanilla `boundOrigin`: the point it wanders around.
    bound_origin: AtomicCell<Option<BlockPos>>,
    /// Vanilla `hasLimitedLife` / `limitedLifeTicks`.
    has_limited_life: AtomicBool,
    limited_life_ticks: AtomicI32,
    /// Vanilla `owner`: the summoning mob.
    owner: AtomicCell<Option<Uuid>>,
    /// Vanilla `DATA_FLAGS_ID`.
    flags: AtomicCell<i8>,
}

impl VexEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        *mob_entity
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(VexMoveControl::new());
        let vex = Self {
            mob_entity,
            bound_origin: AtomicCell::new(None),
            has_limited_life: AtomicBool::new(false),
            limited_life_ticks: AtomicI32::new(0),
            owner: AtomicCell::new(None),
            flags: AtomicCell::new(0),
        };
        let mob_arc = Arc::new(vex);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(4, Box::new(VexChargeAttackGoal));
            goal_selector.add_goal(8, Box::new(VexRandomMoveGoal));
            goal_selector.add_goal(
                9,
                Box::new(LookAtEntityGoal::new(
                    mob_weak.clone(),
                    &EntityType::PLAYER,
                    3.0,
                    1.0,
                    false,
                )),
            );
            goal_selector.add_goal(
                10,
                Box::new(LookAtEntityGoal::matching(
                    mob_weak,
                    TargetMatch::Mob,
                    8.0,
                    0.02,
                    false,
                )),
            );

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(1, raider::revenge_goal());
            target_selector.add_goal(2, Box::new(VexCopyOwnerTargetGoal::new()));
            hunting::apply(target_selector, &mob_arc.mob_entity, VEX_HUNTS);
        };

        mob_arc
    }

    /// Vanilla `setOwner`.
    pub fn set_owner(&self, owner: Option<Uuid>) {
        self.owner.store(owner);
    }

    /// Vanilla `getOwner`: the owner, resolved in the vex's world.
    #[must_use]
    pub fn get_owner(&self) -> Option<Arc<dyn EntityBase>> {
        let uuid = self.owner.load()?;
        self.get_entity().world.load().get_entity_by_uuid(uuid)
    }

    /// Vanilla `setBoundOrigin`.
    pub fn set_bound_origin(&self, origin: Option<BlockPos>) {
        self.bound_origin.store(origin);
    }

    #[must_use]
    pub fn get_bound_origin(&self) -> Option<BlockPos> {
        self.bound_origin.load()
    }

    /// Vanilla `setLimitedLife`.
    pub fn set_limited_life(&self, ticks: i32) {
        self.has_limited_life.store(true, Ordering::Relaxed);
        self.limited_life_ticks.store(ticks, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_charging(&self) -> bool {
        self.flags.load() & FLAG_IS_CHARGING != 0
    }

    /// Vanilla `setIsCharging`, synced so the client shows the charging pose.
    pub fn set_is_charging(&self, charging: bool) {
        let flags = self.flags.load();
        let flags = if charging {
            flags | FLAG_IS_CHARGING
        } else {
            flags & !FLAG_IS_CHARGING
        };
        self.flags.store(flags);
        self.get_entity()
            .set_synced_data(pumpkin_data::tracked_data::vex::DATA_FLAGS_ID, flags);
    }
}

/// The vex driving a goal or control, or `None` for any other mob.
fn as_vex(mob: &dyn Mob) -> Option<&VexEntity> {
    mob.get_entity()
        .entity_type
        .eq(&EntityType::VEX)
        .then(|| mob.cast_any().downcast_ref::<VexEntity>())
        .flatten()
}

impl Mob for VexEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_is_flapping(&self) -> bool {
        self.get_entity().tick_count.load(Ordering::Relaxed) % TICKS_PER_FLAP == 0
    }

    fn get_mob_gravity(&self) -> f64 {
        0.0
    }

    /// Vanilla `tick` runs `super.tick()` with `noPhysics` set: the vex flies through blocks.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.get_entity().no_physics.store(true, Ordering::Relaxed);
    }

    fn post_tick(&self) {
        let entity = self.get_entity();
        entity.no_physics.store(false, Ordering::Relaxed);
        entity.set_has_no_gravity(true);
        if self.has_limited_life.load(Ordering::Relaxed)
            && self.limited_life_ticks.fetch_sub(1, Ordering::Relaxed) - 1 <= 0
        {
            self.limited_life_ticks.store(20, Ordering::Relaxed);
            self.damage(self, 1.0, DamageType::STARVE);
        }
    }

    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(Sound::EntityVexAmbient)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        _reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        // Vanilla `populateDefaultEquipmentSlots`: an iron sword that never drops.
        self.mob_entity.set_item_slot(
            &EquipmentSlot::MAIN_HAND,
            ItemStack::new(1, &Item::IRON_SWORD),
        );
        self.mob_entity
            .living_entity
            .equipment_drop_chances
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(EquipmentSlot::MAIN_HAND, 0.0);
        self.default_finalize_spawn(world, group_data)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(origin) = self.bound_origin.load() {
            nbt.put(
                "bound_pos",
                NbtTag::IntArray(vec![origin.0.x, origin.0.y, origin.0.z]),
            );
        }
        if self.has_limited_life.load(Ordering::Relaxed) {
            nbt.put_int(
                "life_ticks",
                self.limited_life_ticks.load(Ordering::Relaxed),
            );
        }
        if let Some(owner) = self.owner.load() {
            nbt.put_uuid("owner", owner);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.bound_origin
            .store(nbt.get_int_array("bound_pos").and_then(|pos| match pos {
                [x, y, z] => Some(BlockPos::new(*x, *y, *z)),
                _ => None,
            }));
        match nbt.get_int("life_ticks") {
            Some(ticks) => self.set_limited_life(ticks),
            None => self.has_limited_life.store(false, Ordering::Relaxed),
        }
        self.owner.store(nbt.get_uuid("owner"));
    }
}

/// Vanilla `VexMoveControl`: flies straight at the wanted position, ignoring terrain.
pub struct VexMoveControl {
    wanted: Vector3<f64>,
    speed_modifier: f64,
    operation: Operation,
}

impl VexMoveControl {
    const fn new() -> Self {
        Self {
            wanted: Vector3::new(0.0, 0.0, 0.0),
            speed_modifier: 0.0,
            operation: Operation::Wait,
        }
    }
}

impl Control for VexMoveControl {}

impl MoveControlTrait for VexMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        if self.operation != Operation::MoveTo {
            return;
        }
        let entity = mob.get_entity();
        let delta = self.wanted - entity.pos.load();
        let distance = delta.length();
        let bbox = entity.bounding_box.load();
        // Vanilla `AABB.getSize`: the mean edge length.
        let size =
            ((bbox.max.x - bbox.min.x) + (bbox.max.y - bbox.min.y) + (bbox.max.z - bbox.min.z))
                / 3.0;
        if distance < size {
            self.operation = Operation::Wait;
            entity.velocity.store(entity.velocity.load() * 0.5);
            return;
        }

        let velocity = entity.velocity.load() + delta * (self.speed_modifier * 0.05 / distance);
        entity.velocity.store(velocity);
        // Face the target if there is one, otherwise the direction of travel.
        let facing = mob
            .get_mob_entity()
            .get_target()
            .map_or(velocity, |target| {
                target.get_entity().pos.load() - entity.pos.load()
            });
        let yaw = -(facing.x.atan2(facing.z) as f32) * 57.295_776;
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.wanted = Vector3::new(x, y, z);
        self.speed_modifier = speed_modifier;
        if self.operation != Operation::Jumping {
            self.operation = Operation::MoveTo;
        }
    }

    fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }

    fn get_speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    fn get_wanted_position(&self) -> Vector3<f64> {
        self.wanted
    }
}

fn has_wanted(mob: &dyn Mob) -> bool {
    mob.get_mob_entity()
        .move_control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .has_wanted()
}

fn set_wanted(mob: &dyn Mob, pos: Vector3<f64>, speed: f64) {
    mob.get_mob_entity()
        .move_control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .set_wanted_position(pos.x, pos.y, pos.z, speed);
}

/// Vanilla `VexChargeAttackGoal`: lunges at the target's eyes and strikes on contact.
struct VexChargeAttackGoal;

impl Goal for VexChargeAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        target.get_entity().is_alive()
            && !has_wanted(mob)
            && mob.get_random().random_range(0..to_goal_ticks(7)) == 0
            && mob
                .get_entity()
                .pos
                .load()
                .squared_distance_to_vec(&target.get_entity().pos.load())
                > 4.0
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        has_wanted(mob)
            && as_vex(mob).is_some_and(VexEntity::is_charging)
            && mob
                .get_mob_entity()
                .get_target()
                .is_some_and(|target| target.get_entity().is_alive())
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(target) = mob.get_mob_entity().get_target() {
            set_wanted(mob, target.get_entity().get_eye_pos(), 1.0);
        }
        if let Some(vex) = as_vex(mob) {
            vex.set_is_charging(true);
        }
        let entity = mob.get_entity();
        entity.world.load().play_sound_fine(
            Sound::EntityVexCharge,
            SoundCategory::Hostile,
            &entity.pos.load(),
            1.0,
            1.0,
        );
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(vex) = as_vex(mob) {
            vex.set_is_charging(false);
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };
        let entity = mob.get_entity();
        let target_entity = target.get_entity();
        if entity
            .bounding_box
            .load()
            .intersects(&target_entity.bounding_box.load())
        {
            mob.do_hurt_target(target.as_ref());
            if let Some(vex) = as_vex(mob) {
                vex.set_is_charging(false);
            }
        } else if entity
            .pos
            .load()
            .squared_distance_to_vec(&target_entity.pos.load())
            < 9.0
        {
            set_wanted(mob, target_entity.get_eye_pos(), 1.0);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `VexRandomMoveGoal`: drifts to a random empty block near its bound origin.
struct VexRandomMoveGoal;

impl Goal for VexRandomMoveGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !has_wanted(mob) && mob.get_random().random_range(0..to_goal_ticks(7)) == 0
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        false
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        let origin = as_vex(mob)
            .and_then(VexEntity::get_bound_origin)
            .unwrap_or_else(|| entity.block_pos.load());
        let world = entity.world.load();
        for _ in 0..3 {
            let mut rng = mob.get_random();
            let pos = BlockPos::new(
                origin.0.x + rng.random_range(0..15) - 7,
                origin.0.y + rng.random_range(0..11) - 5,
                origin.0.z + rng.random_range(0..15) - 7,
            );
            drop(rng);
            if !world.get_block_state(&pos).is_air() {
                continue;
            }
            let center = Vector3::new(
                f64::from(pos.0.x) + 0.5,
                f64::from(pos.0.y) + 0.5,
                f64::from(pos.0.z) + 0.5,
            );
            set_wanted(mob, center, 0.25);
            if mob.get_mob_entity().get_target().is_none() {
                mob.get_mob_entity()
                    .look_control
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .look_at_with_range(center.x, center.y, center.z, 180.0, 20.0);
            }
            break;
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `VexCopyOwnerTargetGoal`: takes on whatever its summoner is targeting.
struct VexCopyOwnerTargetGoal {
    track_target_goal: TrackTargetGoal,
    copy_owner_targeting: TargetPredicate,
    target: Option<Arc<dyn EntityBase>>,
}

impl VexCopyOwnerTargetGoal {
    fn new() -> Self {
        Self {
            track_target_goal: TrackTargetGoal::with_default(false),
            // Vanilla `forNonCombat().ignoreLineOfSight().ignoreInvisibilityTesting()`.
            copy_owner_targeting: TargetPredicate::create_non_attackable()
                .ignore_visibility()
                .ignore_distance_scaling_factor(),
            target: None,
        }
    }

    fn owner_target(mob: &dyn Mob) -> Option<Arc<dyn EntityBase>> {
        let owner = as_vex(mob)?.get_owner()?;
        owner.get_mob()?.get_mob_entity().get_target()
    }
}

impl Goal for VexCopyOwnerTargetGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(target) = Self::owner_target(mob) else {
            return false;
        };
        if !self
            .track_target_goal
            .can_track(mob, Some(target.as_ref()), &self.copy_owner_targeting)
        {
            return false;
        }
        self.target = Some(target);
        true
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.track_target_goal.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.track_target_goal
            .start_targeting(mob, Self::owner_target(mob), TargetReason::Unknown);
        self.track_target_goal.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.target = None;
        self.track_target_goal.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.track_target_goal.controls()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::ai::test_support::world;

    fn spawn(world: &Arc<World>) -> Arc<VexEntity> {
        let vex = VexEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 80.0, 0.5),
            &EntityType::VEX,
        ));
        world.spawn_entity(vex.clone());
        vex
    }

    #[tokio::test]
    async fn move_control_flies_straight_at_the_wanted_point()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let vex = spawn(&world);
        let mut control = VexMoveControl::new();
        control.set_wanted_position(10.5, 80.0, 0.5, 1.0);
        control.tick(&*vex);
        let velocity = vex.get_entity().velocity.load();
        world.level.shutdown().await;

        // One tick adds speed * 0.05 along the unit direction.
        assert!((velocity.x - 0.05).abs() < 1.0e-9, "{velocity:?}");
        assert!(velocity.y.abs() < 1.0e-9 && velocity.z.abs() < 1.0e-9);
        assert!(control.has_wanted());
        Ok(())
    }

    #[tokio::test]
    async fn limited_life_starves_and_nbt_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let vex = spawn(&world);
        vex.set_bound_origin(Some(BlockPos::new(3, 70, -2)));
        vex.set_limited_life(600);
        let owner = Uuid::new_v4();
        vex.set_owner(Some(owner));

        let mut nbt = NbtCompound::new();
        vex.mob_write_nbt(&mut nbt);
        let loaded = spawn(&world);
        loaded.mob_read_nbt(&nbt);
        world.level.shutdown().await;

        assert_eq!(loaded.get_bound_origin(), Some(BlockPos::new(3, 70, -2)));
        assert!(loaded.has_limited_life.load(Ordering::Relaxed));
        assert_eq!(loaded.limited_life_ticks.load(Ordering::Relaxed), 600);
        assert_eq!(loaded.owner.load(), Some(owner));
        Ok(())
    }

    #[tokio::test]
    async fn charging_flag_is_synced() -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let vex = spawn(&world);
        vex.set_is_charging(true);
        let charging = vex.is_charging();
        vex.set_is_charging(false);
        let stopped = vex.is_charging();
        world.level.shutdown().await;
        assert!(charging && !stopped);
        Ok(())
    }
}
