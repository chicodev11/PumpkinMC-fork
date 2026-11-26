use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::EntityBase;
use crate::entity::ai::goal::{Controls, Goal};
use crate::entity::mob::raider::create_ominous_banner;
use crate::entity::mob::spawn::EntitySpawnReason;
use crate::entity::mob::{Mob, MobEntity};
use crate::world::World;
use pumpkin_world::chunk::ChunkHeightmapType;
use std::sync::Arc;

pub struct PatrolData {
    pub patrol_target: AtomicCell<Option<BlockPos>>,
    pub patrol_leader: AtomicBool,
    pub patrolling: AtomicBool,
}

impl Default for PatrolData {
    fn default() -> Self {
        Self {
            patrol_target: AtomicCell::new(None),
            patrol_leader: AtomicBool::new(false),
            patrolling: AtomicBool::new(false),
        }
    }
}

/// Puts the ominous banner on a patrol or raid leader's head with a guaranteed drop.
pub fn equip_leader_banner(mob: &MobEntity) {
    mob.set_item_slot(&EquipmentSlot::HEAD, create_ominous_banner());
    mob.set_guaranteed_drop(&EquipmentSlot::HEAD);
}

pub trait PatrollingMonster: Mob {
    fn get_patrol_data(&self) -> &PatrolData;

    fn can_be_leader(&self) -> bool {
        true
    }

    fn is_patrol_leader(&self) -> bool {
        self.get_patrol_data().patrol_leader.load(Ordering::Relaxed)
    }

    fn set_patrol_leader(&self, is_leader: bool) {
        self.get_patrol_data()
            .patrol_leader
            .store(is_leader, Ordering::Relaxed);
        self.set_patrolling(true);
    }

    fn is_patrolling(&self) -> bool {
        self.get_patrol_data().patrolling.load(Ordering::Relaxed)
    }

    fn set_patrolling(&self, patrolling: bool) {
        self.get_patrol_data()
            .patrolling
            .store(patrolling, Ordering::Relaxed);
    }

    fn get_patrol_target(&self) -> Option<BlockPos> {
        self.get_patrol_data().patrol_target.load()
    }

    fn set_patrol_target(&self, target: BlockPos) {
        self.get_patrol_data().patrol_target.store(Some(target));
        self.set_patrolling(true);
    }

    fn has_patrol_target(&self) -> bool {
        self.get_patrol_data().patrol_target.load().is_some()
    }

    fn can_join_patrol(&self) -> bool {
        true
    }

    /// Vanilla `findPatrolTarget`: a point up to 500 blocks away on each axis.
    fn find_patrol_target(&self) {
        let block_pos = self.get_mob_entity().living_entity.entity.block_pos.load();
        let target = block_pos.offset(Vector3::new(
            -500 + rand::random_range(0..1000),
            0,
            -500 + rand::random_range(0..1000),
        ));
        self.get_patrol_data().patrol_target.store(Some(target));
        self.set_patrolling(true);
    }

    /// The patrol part of vanilla `PatrollingMonster.finalizeSpawn`: natural spawns may become
    /// leaders, leaders wear an ominous banner, and patrol spawns start patrolling.
    fn finalize_patrol_spawn(&self, reason: EntitySpawnReason) {
        if !matches!(
            reason,
            EntitySpawnReason::Patrol | EntitySpawnReason::Event | EntitySpawnReason::Structure
        ) && rand::random::<f32>() < 0.06
            && self.can_be_leader()
        {
            self.get_patrol_data()
                .patrol_leader
                .store(true, Ordering::Relaxed);
        }
        if self.is_patrol_leader() {
            equip_leader_banner(self.get_mob_entity());
        }
        if reason == EntitySpawnReason::Patrol {
            self.set_patrolling(true);
        }
    }

    fn write_patrol_nbt(&self, nbt: &mut NbtCompound) {
        let data = self.get_patrol_data();
        if let Some(target) = data.patrol_target.load() {
            nbt.put_int("PatrolTargetX", target.0.x);
            nbt.put_int("PatrolTargetY", target.0.y);
            nbt.put_int("PatrolTargetZ", target.0.z);
        }
        nbt.put_bool("PatrolLeader", data.patrol_leader.load(Ordering::Relaxed));
        nbt.put_bool("Patrolling", data.patrolling.load(Ordering::Relaxed));
    }

    fn read_patrol_nbt(&self, nbt: &NbtCompound) {
        let data = self.get_patrol_data();
        if let (Some(x), Some(y), Some(z)) = (
            nbt.get_int("PatrolTargetX"),
            nbt.get_int("PatrolTargetY"),
            nbt.get_int("PatrolTargetZ"),
        ) {
            data.patrol_target
                .store(Some(BlockPos(Vector3::new(x, y, z))));
        }
        if let Some(leader) = nbt.get_bool("PatrolLeader") {
            data.patrol_leader.store(leader, Ordering::Relaxed);
        }
        if let Some(patrolling) = nbt.get_bool("Patrolling") {
            data.patrolling.store(patrolling, Ordering::Relaxed);
        }
    }
}

/// Vanilla `PatrollingMonster.LongDistancePatrolGoal`: the leader walks toward the patrol
/// target in 10-block steps and hands each step to its companions.
pub struct LongDistancePatrolGoal {
    speed_modifier: f64,
    leader_speed_modifier: f64,
    cooldown_until: i64,
}

impl LongDistancePatrolGoal {
    const NAVIGATION_FAILED_COOLDOWN: i64 = 200;

    #[must_use]
    pub const fn new(speed_modifier: f64, leader_speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            leader_speed_modifier,
            cooldown_until: -1,
        }
    }

    /// Patrolling monsters within 16 blocks that can join the patrol.
    fn find_patrol_companions(mob: &dyn Mob) -> Vec<Arc<dyn EntityBase>> {
        let entity = mob.get_entity();
        let world = entity.world.load();
        world
            .get_entities_at_box(&entity.bounding_box.load().expand_all(16.0))
            .into_iter()
            .filter(|other| {
                other.get_entity().entity_id != entity.entity_id
                    && other
                        .get_mob()
                        .and_then(Mob::as_patrolling_monster)
                        .is_some_and(PatrollingMonster::can_join_patrol)
            })
            .collect()
    }

    /// Vanilla `moveRandomly`: a surface spot up to 8 blocks away.
    fn move_randomly(&self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        let world = entity.world.load();
        let pos = entity.block_pos.load();
        let x = pos.0.x - 8 + rand::random_range(0..16);
        let z = pos.0.z - 8 + rand::random_range(0..16);
        let y = surface_y(&world, x, z);
        let steered = mob.get_mob_entity().steered();
        steered.navigator().move_to_coords(
            f64::from(x),
            f64::from(y),
            f64::from(z),
            self.speed_modifier,
            &steered.mob_entity().living_entity,
        )
    }
}

/// `Level.getHeightmapPos(MOTION_BLOCKING_NO_LEAVES, ..).getY()`.
fn surface_y(world: &World, x: i32, z: i32) -> i32 {
    world.get_heightmap_height(ChunkHeightmapType::MotionBlockingNoLeaves, x, z) + 1
}

impl Goal for LongDistancePatrolGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(patrol) = mob.as_patrolling_monster() else {
            return false;
        };
        let on_cooldown = mob.get_entity().world.load().get_world_age() < self.cooldown_until;
        patrol.is_patrolling()
            && mob.get_mob_entity().get_target().is_none()
            && mob.mob_controlling_passenger().is_none()
            && patrol.has_patrol_target()
            && !on_cooldown
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(patrol) = mob.as_patrolling_monster() else {
            return;
        };
        let is_leader = patrol.is_patrol_leader();
        if !mob.get_mob_entity().steered().navigator().is_done() {
            return;
        }
        let companions = Self::find_patrol_companions(mob);
        if patrol.is_patrolling() && companions.is_empty() {
            patrol.set_patrolling(false);
            return;
        }
        let Some(patrol_target) = patrol.get_patrol_target() else {
            return;
        };
        let entity = mob.get_entity();
        let position = entity.pos.load();
        if is_leader
            && patrol_target
                .to_centered_f64()
                .squared_distance_to_vec(&position)
                < 100.0
        {
            patrol.find_patrol_target();
            return;
        }
        // Aim 10 blocks toward the target, offset sideways; `yRot` takes radians, so the
        // 90 here really is 90 radians.
        let target = Vector3::new(
            f64::from(patrol_target.0.x) + 0.5,
            f64::from(patrol_target.0.y),
            f64::from(patrol_target.0.z) + 0.5,
        );
        let offset = position.sub(&target);
        let (sin, cos) = 90.0f32.sin_cos();
        let rotated = Vector3::new(
            offset.x * f64::from(cos) + offset.z * f64::from(sin),
            offset.y,
            offset.z * f64::from(cos) - offset.x * f64::from(sin),
        );
        let target = rotated.multiply(0.4, 0.4, 0.4).add(&target);
        let step = target
            .sub(&position)
            .normalize()
            .multiply(10.0, 10.0, 10.0)
            .add(&position);
        let step_x = step.x.floor() as i32;
        let step_z = step.z.floor() as i32;
        let world = entity.world.load();
        let step_pos = BlockPos::new(step_x, surface_y(&world, step_x, step_z), step_z);
        let speed = if is_leader {
            self.leader_speed_modifier
        } else {
            self.speed_modifier
        };
        let moved = {
            let steered = mob.get_mob_entity().steered();
            steered.navigator().move_to_coords(
                f64::from(step_pos.0.x),
                f64::from(step_pos.0.y),
                f64::from(step_pos.0.z),
                speed,
                &steered.mob_entity().living_entity,
            )
        };
        if !moved {
            self.move_randomly(mob);
            self.cooldown_until = world.get_world_age() + Self::NAVIGATION_FAILED_COOLDOWN;
        } else if is_leader {
            for companion in &companions {
                if let Some(companion) = companion.get_mob().and_then(Mob::as_patrolling_monster) {
                    companion.set_patrol_target(step_pos);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::entity::EntityType;

    use super::*;
    use crate::entity::Entity;
    use crate::entity::ai::test_support::world;
    use crate::entity::mob::pillager::PillagerEntity;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn pillager(world: &Arc<World>) -> Arc<PillagerEntity> {
        PillagerEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::PILLAGER,
        ))
    }

    /// Vanilla `PatrollingMonster.finalizeSpawn`: patrol spawns patrol, event spawns never
    /// roll leadership, natural spawns sometimes lead and then carry the banner.
    #[tokio::test]
    async fn patrol_finalize_follows_the_spawn_reason() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let patrol = pillager(&world);
        patrol.finalize_patrol_spawn(EntitySpawnReason::Patrol);
        assert!(patrol.is_patrolling());
        for _ in 0..200 {
            let event = pillager(&world);
            event.finalize_patrol_spawn(EntitySpawnReason::Event);
            assert!(!event.is_patrol_leader() && !event.is_patrolling());
        }
        let leader = (0..400)
            .map(|_| pillager(&world))
            .find(|natural| {
                natural.finalize_patrol_spawn(EntitySpawnReason::Natural);
                natural.is_patrol_leader()
            })
            .ok_or("no natural pillager became a leader")?;
        let mob = leader.get_mob_entity();
        let head = mob
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::HEAD)
            .item
            .id;
        assert_eq!(head, pumpkin_data::item::Item::WHITE_BANNER.id);
        assert!((mob.drop_chance(&EquipmentSlot::HEAD) - 2.0).abs() < f32::EPSILON);
        world.level.shutdown().await;
        Ok(())
    }

    /// A patroller with no companions within 16 blocks stops patrolling.
    #[tokio::test]
    async fn lone_patroller_stops_patrolling() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let lone = pillager(&world);
        lone.set_patrol_target(BlockPos::new(100, 64, 100));
        let mut goal = LongDistancePatrolGoal::new(0.7, 0.595);
        assert!(goal.can_start(lone.as_ref()));
        goal.tick(lone.as_ref());
        assert!(!lone.is_patrolling());
        world.level.shutdown().await;
        Ok(())
    }
}
