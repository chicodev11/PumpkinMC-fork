use core::f32;
use std::sync::{
    Arc,
    atomic::{AtomicI32, AtomicU32, Ordering},
};

use pumpkin_data::{damage::DamageType, entity::EntityType, tag::Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::{
    boundingbox::BoundingBox,
    vector3::{Axis, Vector3},
};

use crate::{
    block::blocks::bubble_column::SURFACE_UPWARD_ACCELERATION,
    server::Server,
    world::{
        World,
        entity_islands::{self, AuditedReach},
    },
};

use super::{Entity, EntityBase, living::LivingEntity, player::Player, weak_slot::WeakSlot};

pub struct ExperienceOrbEntity {
    entity: Entity,
    amount: AtomicU32,
    /// Vanilla `count`: how many orbs of `amount` this entity stands for. Zero once it was
    /// merged away or fully picked up, so concurrent mergers never revive it.
    count: AtomicI32,
    health: AtomicI32,
    orb_age: AtomicU32,
    following_player: WeakSlot<Player>,
}

impl ExperienceOrbEntity {
    const AIR_DRAG: f32 = 0.98;
    const DEFAULT_HEALTH: i32 = 5;
    const LIFETIME: u32 = 6000;
    const ENTITY_SCAN_PERIOD: i32 = 20;
    const ORB_GROUPS_PER_AREA: i32 = 40;
    const ORB_MERGE_DISTANCE: f64 = 0.5;

    pub fn new(entity: Entity, amount: u32) -> Self {
        entity.yaw.store(rand::random::<f32>() * 360.0);
        Self {
            entity,
            amount: AtomicU32::new(amount),
            count: AtomicI32::new(1),
            health: AtomicI32::new(Self::DEFAULT_HEALTH),
            orb_age: AtomicU32::new(0),
            following_player: WeakSlot::new(),
        }
    }

    pub fn spawn(world: &Arc<World>, position: Vector3<f64>, amount: u32) {
        Self::spawn_with_direction(world, position, Vector3::new(0.0, 0.0, 0.0), amount);
    }

    fn new_with_direction(
        world: &Arc<World>,
        position: Vector3<f64>,
        direction: Vector3<f64>,
        amount: u32,
    ) -> Self {
        let entity = Entity::new(world.clone(), position, &EntityType::EXPERIENCE_ORB);
        let orb = Self::new(entity, amount);
        let mut velocity = Vector3::new(
            (rand::random::<f64>() * 0.2 - 0.1) * 2.0,
            rand::random::<f64>() * 0.2 * 2.0,
            (rand::random::<f64>() * 0.2 - 0.1) * 2.0,
        );
        if direction.length_squared() > 0.0 && direction.dot(&velocity) < 0.0 {
            velocity = velocity.multiply(-1.0, -1.0, -1.0);
        }
        let size = orb.entity.bounding_box.load().get_average_side_length();
        let offset = direction.normalize() * (size * 0.5);
        orb.entity.set_pos(position.add(&offset));
        orb.entity.set_velocity(velocity);
        if !world.is_space_empty(orb.entity.bounding_box.load()) {
            orb.unstuck_if_possible(size);
        }
        orb
    }

    fn unstuck_if_possible(&self, max_distance: f64) {
        let bounding_box = self.entity.bounding_box.load();
        let size = bounding_box.max - bounding_box.min;
        let center = (bounding_box.min + bounding_box.max) * 0.5;
        let half_distance = Vector3::new(max_distance, max_distance, max_distance) * 0.5;
        let allowed_centers = BoundingBox {
            min: center - half_distance,
            max: center + half_distance,
        };
        let world = self.entity.world.load();
        let (mut collisions, _) =
            world.get_block_collisions(allowed_centers.expand(size.x, size.y, size.z), self);
        {
            let border = world
                .worldborder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let half = border.new_diameter * 0.5;
            let epsilon = f64::from(1.0e-5f32);
            collisions.retain(|shape| {
                shape.min.x >= border.center_x - half
                    && shape.max.x - epsilon < border.center_x + half
                    && shape.min.z >= border.center_z - half
                    && shape.max.z - epsilon < border.center_z + half
            });
        };
        let expanded_collisions = collisions
            .into_iter()
            .map(|shape| shape.expand(size.x * 0.5, size.y * 0.5, size.z * 0.5));
        if let Some(position) = closest_free_center(allowed_centers, center, expanded_collisions) {
            self.entity
                .set_pos(position.add_raw(0.0, -size.y * 0.5, 0.0));
        }
    }

    pub fn spawn_with_direction(
        world: &Arc<World>,
        position: Vector3<f64>,
        direction: Vector3<f64>,
        amount: u32,
    ) {
        let mut amount = amount;
        while amount > 0 {
            let i = Self::round_to_orb_size(amount);
            amount -= i;
            if !Self::try_merge_to_existing(world, position, i) {
                let orb = Arc::new(Self::new_with_direction(world, position, direction, i));
                world.spawn_entity(orb);
            }
        }
    }

    #[must_use]
    pub fn value(&self) -> u32 {
        self.amount.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn count(&self) -> i32 {
        self.count.load(Ordering::Relaxed)
    }

    /// Vanilla `canMerge(orb, id, value)`: one of 40 id groups, with the same value.
    fn can_merge_with(other: &Self, id: i32, value: u32) -> bool {
        !other.entity.is_removed()
            && (other.entity.entity_id - id) % Self::ORB_GROUPS_PER_AREA == 0
            && other.value() == value
    }

    /// Vanilla `tryMergeToExisting`: add the new orb to a matching one within half a block.
    fn try_merge_to_existing(world: &Arc<World>, position: Vector3<f64>, value: u32) -> bool {
        let area = BoundingBox::new(
            position.add_raw(-0.5, -0.5, -0.5),
            position.add_raw(0.5, 0.5, 0.5),
        );
        let id = rand::random_range(0..Self::ORB_GROUPS_PER_AREA);
        let entities = world.get_entities_at_box(&area);
        // Vanilla only looks at the first match.
        let Some(orb) = entities.iter().find_map(|entity| {
            entity
                .cast_any()
                .downcast_ref::<Self>()
                .filter(|orb| Self::can_merge_with(orb, id, value))
        }) else {
            return false;
        };
        // A match merged away meanwhile spawns a fresh orb instead.
        let merged = orb
            .count
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |count| {
                (count > 0).then_some(count + 1)
            })
            .is_ok();
        if merged {
            orb.orb_age.store(0, Ordering::Relaxed);
        }
        merged
    }

    /// Vanilla `scanForMerges`: absorb matching orbs within half a block.
    fn scan_for_merges(&self) {
        let entity = &self.entity;
        let area = entity.bounding_box.load().expand(
            Self::ORB_MERGE_DISTANCE,
            Self::ORB_MERGE_DISTANCE,
            Self::ORB_MERGE_DISTANCE,
        );
        let value = self.value();
        for other in entity.world.load().get_entities_at_box(&area) {
            let Some(orb) = other.cast_any().downcast_ref::<Self>() else {
                continue;
            };
            if orb.entity.entity_id == entity.entity_id
                || !Self::can_merge_with(orb, entity.entity_id, value)
            {
                continue;
            }
            self.merge(orb);
        }
    }

    /// Vanilla `merge`: take over the other orb's count and the younger age.
    fn merge(&self, other: &Self) {
        let taken = other.count.swap(0, Ordering::AcqRel);
        if taken <= 0 {
            return;
        }
        // Our own count may have been taken meanwhile; hand the orbs back then.
        if self
            .count
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |count| {
                (count > 0).then_some(count + taken)
            })
            .is_err()
        {
            other.count.store(taken, Ordering::Release);
            return;
        }
        self.orb_age
            .fetch_min(other.orb_age.load(Ordering::Relaxed), Ordering::Relaxed);
        other.entity.remove();
    }

    /// Vanilla `setUnderwaterMovement`: drift up slowly and lose horizontal speed.
    fn set_underwater_movement(&self) {
        let velocity = self.entity.velocity.load();
        self.entity.velocity.store(Vector3::new(
            velocity.x * f64::from(0.99f32),
            (velocity.y + f64::from(5.0e-4f32)).min(f64::from(0.06f32)),
            velocity.z * f64::from(0.99f32),
        ));
    }

    /// Vanilla `ExperienceOrb.followNearbyPlayer`
    /// accelerate towards a player within 8 blocks.
    fn follow_nearby_player(&self) {
        let entity = &self.entity;
        let pos = entity.pos.load();
        let lost = self.following_player.get().is_none_or(|player| {
            player.is_spectator()
                || player.get_entity().pos.load().squared_distance_to_vec(&pos) > 64.0
        });
        if lost {
            let nearest = entity.world.load().get_nearest_player(pos, 8.0, |player| {
                !player.is_spectator() && !player.living_entity.is_dead_or_dying()
            });
            match nearest {
                Some(player) => self.following_player.set(&player),
                None => self.following_player.clear(),
            }
        }
        let Some(player) = self.following_player.get() else {
            return;
        };

        let target = player.get_entity();
        let target_pos = target
            .pos
            .load()
            .add_raw(0.0, target.get_eye_height() / 2.0, 0.0);
        let delta = target_pos.sub(&pos);
        let power = 1.0 - delta.length() / 8.0;
        entity
            .velocity
            .store(entity.velocity.load() + delta.normalize() * (power * power * 0.1));
        entity.velocity_dirty.store(true, Ordering::SeqCst);
    }

    const fn round_to_orb_size(value: u32) -> u32 {
        if value >= 2477 {
            2477
        } else if value >= 1237 {
            1237
        } else if value >= 617 {
            617
        } else if value >= 307 {
            307
        } else if value >= 149 {
            149
        } else if value >= 73 {
            73
        } else if value >= 37 {
            37
        } else if value >= 17 {
            17
        } else if value >= 7 {
            7
        } else if value >= 3 {
            3
        } else {
            1
        }
    }
}

fn closest_free_center(
    allowed_centers: BoundingBox,
    center: Vector3<f64>,
    collisions: impl Iterator<Item = BoundingBox>,
) -> Option<Vector3<f64>> {
    // Subtract expanded collision boxes, matching vanilla's ONLY_FIRST shape operation.
    let mut free_boxes = vec![allowed_centers];
    for collision in collisions {
        let mut remaining = Vec::new();
        for mut free in free_boxes {
            if !free.intersects(&collision) {
                remaining.push(free);
                continue;
            }
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                if free.min.get_axis(axis) < collision.min.get_axis(axis) {
                    let mut piece = free;
                    piece.max.set_axis(axis, collision.min.get_axis(axis));
                    remaining.push(piece);
                    free.min.set_axis(axis, collision.min.get_axis(axis));
                }
                if free.max.get_axis(axis) > collision.max.get_axis(axis) {
                    let mut piece = free;
                    piece.min.set_axis(axis, collision.max.get_axis(axis));
                    remaining.push(piece);
                    free.max.set_axis(axis, collision.max.get_axis(axis));
                }
            }
        }
        free_boxes = remaining;
    }
    free_boxes
        .into_iter()
        .map(|free| {
            Vector3::new(
                center.x.clamp(free.min.x, free.max.x),
                center.y.clamp(free.min.y, free.max.y),
                center.z.clamp(free.min.z, free.max.z),
            )
        })
        .min_by(|a, b| {
            a.squared_distance_to_vec(&center)
                .total_cmp(&b.squared_distance_to_vec(&center))
        })
}

impl EntityBase for ExperienceOrbEntity {
    /// Vanilla `ExperienceOrb.doWaterSplashEffect` does nothing.
    fn do_water_splash_effect(&self, _caller: &dyn EntityBase) {}

    fn audited_reach(&self) -> Option<AuditedReach> {
        let entity = &self.entity;
        let world = entity.world.load();
        let fluid_push = super::max_fluid_push_speed(world.dimension.fast_lava);
        let velocity = entity.velocity.load();
        let mut reach = AuditedReach {
            bounding_box: entity.bounding_box.load(),
            motion: (
                velocity.x.hypot(velocity.z) + fluid_push,
                velocity.y.abs() + self.get_gravity() + SURFACE_UPWARD_ACCELERATION + fluid_push,
            ),
            entities: Self::ORB_MERGE_DISTANCE,
            players: 0.0,
            block_reads: 1.0,
            route: None,
            changes_blocks: false,
            global: false,
            links: entity_islands::links_of(self),
        };
        reach.scan_contact_blocks(&world);
        Some(reach)
    }

    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_short("Health", self.health.load(Ordering::Relaxed) as i16);
        nbt.put_short("Age", self.orb_age.load(Ordering::Relaxed) as i16);
        nbt.put_short("Value", self.value() as i16);
        nbt.put_int("Count", self.count());
    }

    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        self.health.store(
            nbt.get_short("Health")
                .map_or(Self::DEFAULT_HEALTH, i32::from),
            Ordering::Relaxed,
        );
        self.orb_age.store(
            nbt.get_short("Age")
                .map_or(0, |age| u32::try_from(age).unwrap_or(0)),
            Ordering::Relaxed,
        );
        let value = nbt
            .get_short("Value")
            .map_or(0, |value| u32::try_from(value).unwrap_or(0));
        self.amount.store(value, Ordering::Relaxed);
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::experience_orb::VALUE,
            VarInt(value as i32),
        );
        // Vanilla reads `Count` as a positive int, defaulting to 1.
        let count = nbt.get_int("Count").filter(|count| *count > 0).unwrap_or(1);
        self.count.store(count, Ordering::Relaxed);
    }

    fn init_data_tracker(&self) {
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::experience_orb::VALUE,
            VarInt(self.value() as i32),
        );
    }

    fn tick(&self, caller: &dyn EntityBase, server: &Server) {
        let entity = &self.entity;
        entity.tick(caller, server);
        let bounding_box = entity.bounding_box.load();

        let original_velo = entity.velocity.load();

        let mut velo = original_velo;

        let no_physics = !self
            .entity
            .world
            .load()
            .is_space_empty(bounding_box.expand(-1.0e-7, -1.0e-7, -1.0e-7));
        self.entity.no_physics.store(no_physics, Ordering::Relaxed);
        if entity.is_submerged_in_water() {
            self.set_underwater_movement();
            velo = entity.velocity.load();
        } else if !no_physics {
            velo.y -= self.get_gravity();
        }
        entity.velocity.store(velo);

        // Vanilla: an orb in lava pops up with a random horizontal nudge.
        let world = entity.world.load();
        if world
            .get_fluid(&entity.block_pos.load())
            .has_tag(&pumpkin_data::tag::Fluid::MINECRAFT_LAVA)
        {
            let nudge = || f64::from((rand::random::<f32>() - rand::random::<f32>()) * 0.2);
            entity
                .velocity
                .store(Vector3::new(nudge(), f64::from(0.2f32), nudge()));
        }

        if entity.tick_count.load(Ordering::Relaxed) % Self::ENTITY_SCAN_PERIOD == 1 {
            self.scan_for_merges();
        }
        if self.count() <= 0 {
            // Merged into another orb this tick.
            return;
        }
        self.follow_nearby_player();
        let velo = entity.velocity.load();

        entity.move_entity(caller, velo);

        entity.tick_block_collisions(caller);

        let on_ground = entity.on_ground.load(Ordering::Relaxed);
        let mut friction = Self::AIR_DRAG;
        if on_ground {
            let pos = entity.get_pos_with_y_offset(f64::from(0.999_999f32)).0;
            let block = entity.world.load().get_block(&pos);
            friction *= block.slipperiness;
        }
        let mut velocity = entity.velocity.load() * f64::from(friction);
        if on_ground && velo.y < -self.get_gravity() {
            velocity.y = -velo.y * 0.4;
        }
        entity.velocity.store(velocity);

        let age = self.orb_age.fetch_add(1, Ordering::Relaxed) + 1;
        if age >= Self::LIFETIME {
            entity.remove();
        }
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn receives_player_touch(&self) -> bool {
        true
    }

    fn damage_with_context(
        &self,
        _caller: &dyn EntityBase,
        amount: f32,
        damage_type: DamageType,
        _position: Option<Vector3<f64>>,
        _source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        if self.entity.is_invulnerable_to(&damage_type, cause) {
            return false;
        }

        let mut event = crate::plugin::api::events::entity::entity_damage::EntityDamageEvent::new(
            self.entity.entity_id,
            damage_type,
            amount,
        );
        if let Some(server) = self.entity.world.load().server.upgrade() {
            server.plugin_manager.fire_blocking(&server, &mut event);
        }
        if event.cancelled {
            return false;
        }
        if !event.damage.is_finite() || event.damage <= 0.0 {
            return false;
        }

        self.entity.velocity_dirty.store(true, Ordering::Relaxed);
        let mut health = self.health.load(Ordering::Relaxed);
        loop {
            let remaining = (health as f32 - event.damage) as i32;
            match self.health.compare_exchange_weak(
                health,
                remaining,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    if remaining <= 0 {
                        self.entity.remove();
                    }
                    return true;
                }
                Err(current) => health = current,
            }
        }
    }

    fn on_player_collision(&self, player: &Arc<Player>) {
        let can_pickup = if let Ok(mut delay) = player.experience_pick_up_delay.try_lock()
            && *delay == 0
        {
            *delay = 2;
            true
        } else {
            false
        };
        if can_pickup {
            // Bukkit `PlayerExpCooldownChangeEvent` (`PICKUP_ORB`): plugins may set the cooldown.
            let mut event = crate::plugin::api::events::player::player_exp_cooldown_change::PlayerExpCooldownChangeEvent {
                player: player.clone(),
                new_cooldown: 2,
                cancelled: false,
            };
            player.world().fire_plugin_event(&mut event);
            if event.new_cooldown != 2
                && let Ok(mut delay) = player.experience_pick_up_delay.lock()
            {
                *delay = event.new_cooldown.max(0) as _;
            }
        }
        // Vanilla takes one orb of the stack per pickup.
        let taken = can_pickup
            && player.living_entity.pickup_allowed(&self.entity, 1)
            && self
                .count
                .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |count| {
                    (count > 0).then_some(count - 1)
                })
                .is_ok();
        if taken {
            player.living_entity.send_pickup_animation(&self.entity, 1);
            if self.count() == 0 {
                self.entity.remove();
            }
            let amount = self.value() as i32;
            let remaining = player.apply_mending_from_xp(amount);
            if remaining > 0 {
                player.add_experience_points(remaining);
            }
        }
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }
    fn get_gravity(&self) -> f64 {
        0.03
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unstuck_selects_nearest_free_center_and_leaves_enclosed_orbs_in_place() {
        let allowed = BoundingBox {
            min: Vector3::new(0.0, 0.0, 0.0),
            max: Vector3::new(1.0, 1.0, 1.0),
        };
        let center = Vector3::new(0.5, 0.5, 0.5);
        let wall = BoundingBox {
            min: Vector3::new(-1.0, -1.0, -1.0),
            max: Vector3::new(0.75, 2.0, 2.0),
        };
        let floor = BoundingBox {
            min: Vector3::new(-1.0, -1.0, -1.0),
            max: Vector3::new(2.0, 0.625, 2.0),
        };
        assert_eq!(
            closest_free_center(allowed, center, [wall, floor].into_iter()),
            Some(Vector3::new(0.75, 0.625, 0.5))
        );
        assert_eq!(
            closest_free_center(allowed, center, [allowed].into_iter()),
            None
        );
    }

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn orb(world: &Arc<World>, value: u32) -> ExperienceOrbEntity {
        ExperienceOrbEntity::new(
            Entity::new(
                world.clone(),
                Vector3::new(0.0, 64.0, 0.0),
                &EntityType::EXPERIENCE_ORB,
            ),
            value,
        )
    }

    /// Vanilla saves `Health`, `Age`, `Value` and `Count`; a missing or non-positive count is 1.
    #[tokio::test]
    async fn value_age_and_count_round_trip_through_nbt() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = crate::entity::ai::test_support::world(&folder);
        let saved = orb(&world, 37);
        saved.count.store(5, Ordering::Relaxed);
        saved.orb_age.store(1234, Ordering::Relaxed);
        let mut nbt = NbtCompound::new();
        saved.write_custom_nbt(&mut nbt);

        let loaded = orb(&world, 1);
        loaded.read_custom_nbt(&nbt);
        assert_eq!(loaded.value(), 37);
        assert_eq!(loaded.count(), 5);
        assert_eq!(loaded.orb_age.load(Ordering::Relaxed), 1234);

        nbt.put_int("Count", 0);
        loaded.read_custom_nbt(&nbt);
        assert_eq!(loaded.count(), 1);
        Ok(())
    }

    /// Orbs only merge within one of 40 id groups and with an equal value, and a merged-away
    /// orb hands its whole count over exactly once.
    #[tokio::test]
    async fn merging_needs_the_same_group_and_value_and_moves_the_count() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = crate::entity::ai::test_support::world(&folder);
        let a = orb(&world, 7);
        let b = orb(&world, 7);
        let id = a.entity.entity_id;
        assert_eq!(
            ExperienceOrbEntity::can_merge_with(&b, id, 7),
            (b.entity.entity_id - id) % 40 == 0
        );
        assert!(!ExperienceOrbEntity::can_merge_with(
            &b,
            b.entity.entity_id,
            3
        ));
        assert!(ExperienceOrbEntity::can_merge_with(
            &b,
            b.entity.entity_id - 80,
            7
        ));

        b.count.store(3, Ordering::Relaxed);
        b.orb_age.store(10, Ordering::Relaxed);
        a.orb_age.store(500, Ordering::Relaxed);
        a.merge(&b);
        assert_eq!(a.count(), 4);
        assert_eq!(b.count(), 0);
        assert_eq!(a.orb_age.load(Ordering::Relaxed), 10);
        assert!(b.entity.is_removed());
        a.merge(&b);
        assert_eq!(a.count(), 4);
        Ok(())
    }
}
