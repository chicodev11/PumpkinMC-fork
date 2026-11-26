use super::{Controls, Goal};

use crate::entity::EntityBase;
use crate::entity::ai::goal::revenge::MobFilter;
use crate::entity::ai::pathfinder::path::Path;
use crate::entity::mob::Mob;
use crate::entity::predicate::EntityPredicate;
use crate::plugin::api::events::entity::entity_target::TargetReason;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

const MAX_ATTACK_TIME: i64 = 20;

pub struct MeleeAttackGoal {
    goal_control: Controls,
    speed: f64,
    pause_when_mob_idle: bool,
    /// Route found by `can_start`, handed to the navigator in `start`.
    path: Option<Path>,
    update_countdown_ticks: i32,
    pub cooldown: i32,
    #[expect(dead_code)]
    attack_interval_ticks: i32,
    last_update_time: i64,
    last_target_position: Option<Vector3<f64>>,
    gate: Option<MobFilter>,
    attack: Option<fn(&dyn Mob, &dyn EntityBase)>,
}

impl MeleeAttackGoal {
    #[must_use]
    pub fn new(speed: f64, pause_when_mob_idle: bool) -> Self {
        Self {
            goal_control: Controls::MOVE | Controls::LOOK,
            speed,
            pause_when_mob_idle,
            path: None,
            update_countdown_ticks: 0,
            cooldown: 0,
            attack_interval_ticks: 20,
            last_update_time: 0,
            last_target_position: None,
            gate: None,
            attack: None,
        }
    }

    /// Extra condition for starting and for continuing
    #[must_use]
    pub const fn gated_by(mut self, gate: MobFilter) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Replaces the swing and hit, vanilla `checkAndPerformAttack` overrides.
    #[must_use]
    pub const fn performing(mut self, attack: fn(&dyn Mob, &dyn EntityBase)) -> Self {
        self.attack = Some(attack);
        self
    }

    #[must_use]
    pub fn get_max_cooldown(&self) -> i32 {
        self.get_tick_count(20)
    }
}

impl Goal for MeleeAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if self.gate.is_some_and(|gate| !gate(mob)) {
            return false;
        }
        let time = mob.get_entity().world.load().get_world_age();

        if time - self.last_update_time < MAX_ATTACK_TIME {
            return false;
        }
        self.last_update_time = time;

        let target = mob.get_mob_entity().get_target();

        let Some(target) = target.as_ref() else {
            return false;
        };
        if !target.get_entity().is_alive() {
            return false;
        }
        // Vanilla: `path = getNavigation().createPath(target, 0)`; without a route the goal only
        // starts when the target is already within reach.
        let steered = mob.get_mob_entity().steered();
        self.path = steered.navigator().create_path(
            &steered.mob_entity().living_entity,
            target.get_entity().pos.load(),
            0,
        );
        if self.path.is_some() {
            return true;
        }
        mob.get_mob_entity().is_in_attack_range(target.as_ref())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        if self.gate.is_some_and(|gate| !gate(mob)) {
            return false;
        }
        let target = mob.get_mob_entity().get_target().clone();

        let Some(target) = target else {
            return false;
        };
        if !target.get_entity().is_alive() {
            return false;
        }

        if !self.pause_when_mob_idle {
            let is_idle = mob.is_navigator_idle();
            return !is_idle;
        }

        let is_valid_target = !target
            .get_player()
            .is_some_and(|p| p.is_spectator() || p.is_creative());

        let in_range = mob
            .get_mob_entity()
            .is_in_position_target_range_pos(&target.get_entity().block_pos.load());

        in_range && is_valid_target
    }

    fn start(&mut self, mob: &dyn Mob) {
        // Vanilla: `getNavigation().moveTo(path, speedModifier)`. The pathed target position is
        // deliberately kept across runs, like vanilla's `pathedTargetX/Y/Z`.
        let steered = mob.get_mob_entity().steered();
        steered.navigator().move_to_path(
            self.path.take(),
            self.speed,
            &steered.mob_entity().living_entity,
        );
        mob.get_mob_entity().set_attacking(true);
        self.update_countdown_ticks = 0;
        self.cooldown = 0;
    }

    fn stop(&mut self, mob: &dyn Mob) {
        // Only clear target if they switched to creative/spectator
        let should_clear = mob
            .get_mob_entity()
            .get_target()
            .as_deref()
            .is_some_and(|entity| {
                !EntityPredicate::ExceptCreativeOrSpectator.test(entity.get_entity())
            });
        if should_clear {
            mob.set_mob_target(None, TargetReason::ForgotTarget);
        }

        mob.get_mob_entity().set_attacking(false);

        // Vanilla: this.mob.getNavigation().stop()
        mob.get_mob_entity().steered().navigator().stop();
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let target = mob.get_mob_entity().get_target().clone();
        let Some(target) = target else {
            return;
        };

        mob.get_mob_entity()
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(&target, 30.0, 30.0);

        self.update_countdown_ticks = (self.update_countdown_ticks - 1).max(0);

        let current_target_pos = target.get_entity().pos.load();
        let has_line_of_sight =
            self.pause_when_mob_idle || mob.has_line_of_sight(target.get_entity());
        let should_update_nav = has_line_of_sight
            && self.update_countdown_ticks <= 0
            && (self.last_target_position.is_none_or(|last_pos| {
                current_target_pos.squared_distance_to_vec(&last_pos) >= 1.0
            }) || mob.get_random().random::<f32>() < 0.05);

        if should_update_nav {
            let mob_pos = mob.get_entity().pos.load();
            let dist_sq = mob_pos.squared_distance_to_vec(&current_target_pos);
            self.last_target_position = Some(current_target_pos);
            self.update_countdown_ticks = 4 + mob.get_random().random_range(0..7);
            if dist_sq > 1024.0 {
                self.update_countdown_ticks += 10;
            } else if dist_sq > 256.0 {
                self.update_countdown_ticks += 5;
            }
            // Vanilla: `if (!getNavigation().moveTo(target, speedModifier)) delay += 15`.
            let steered = mob.get_mob_entity().steered();
            let mut navigator = steered.navigator();
            let living = &steered.mob_entity().living_entity;
            let path = navigator.create_path(living, current_target_pos, 0);
            if !navigator.move_to_path(path, self.speed, living) {
                self.update_countdown_ticks += 15;
            }
            drop(navigator);
            self.update_countdown_ticks = self.get_tick_count(self.update_countdown_ticks);
        }

        self.cooldown = (self.cooldown - 1).max(0);

        if self.cooldown <= 0
            && mob.is_in_attack_range(target.as_ref())
            && mob.has_line_of_sight(target.get_entity())
        {
            self.cooldown = self.get_max_cooldown();
            if let Some(attack) = self.attack {
                attack(mob, target.as_ref());
                return;
            }

            mob.get_mob_entity().living_entity.swing_hand();
            let damaged = mob.do_hurt_target(target.as_ref());
            if damaged {
                mob.on_attack(target.as_ref());
            }
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::ai::test_support::world;
    use crate::entity::mob::zombie::ZombieEntityBase;
    use crate::entity::passive::pig::PigEntity;
    use crate::entity::{Entity, EntityBase};
    use crate::world::World;
    use pumpkin_data::Block;
    use pumpkin_data::entity::EntityType;
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::math::vector2::Vector2;
    use pumpkin_world::chunk::ChunkData;
    use pumpkin_world::world::BlockFlags;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const FLAGS: BlockFlags =
        BlockFlags::SKIP_BLOCK_ADDED_CALLBACK.union(BlockFlags::UPDATE_KNOWN_SHAPE);

    fn stone_floor(world: &Arc<World>) {
        for chunk_x in -2..=1 {
            for chunk_z in -2..=1 {
                world.level.loaded_chunks.insert(
                    Vector2::new(chunk_x, chunk_z),
                    ChunkData::empty_sync(chunk_x, chunk_z),
                );
            }
        }
        for x in -20..=20 {
            for z in -20..=20 {
                world.set_block_state(
                    &BlockPos::new(x, 63, z),
                    Block::STONE.default_state.id,
                    FLAGS,
                );
            }
        }
        // `MeleeAttackGoal` only checks `canUse` once every 20 game ticks.
        world
            .level_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .world_age = 100;
    }

    fn stone(world: &Arc<World>, x: i32, z: i32) {
        for y in 64..=65 {
            world.set_block_state(
                &BlockPos::new(x, y, z),
                Block::STONE.default_state.id,
                FLAGS,
            );
        }
    }

    fn spawn_pair(world: &Arc<World>, target_x: f64) -> (Arc<ZombieEntityBase>, Arc<PigEntity>) {
        let zombie = ZombieEntityBase::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::ZOMBIE,
        ));
        let pig = PigEntity::new(Entity::new(
            world.clone(),
            Vector3::new(target_x, 64.0, 0.5),
            &EntityType::PIG,
        ));
        for entity in [zombie.get_entity(), pig.get_entity()] {
            entity.on_ground.store(true, Ordering::Relaxed);
        }
        world.spawn_entity(zombie.clone());
        world.spawn_entity(pig.clone());
        zombie.set_mob_target(Some(pig.clone()), TargetReason::Unknown);
        (zombie, pig)
    }

    #[tokio::test]
    async fn start_installs_a_route_around_a_wall() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        stone_floor(&world);
        for z in -3..=3 {
            stone(&world, 3, z);
        }
        let (zombie, _pig) = spawn_pair(&world, 6.5);

        let mut goal = MeleeAttackGoal::new(1.0, false);
        assert!(
            goal.can_start(&*zombie),
            "a reachable target must start the goal"
        );
        assert!(goal.path.is_some(), "can_start keeps the route for start");
        goal.start(&*zombie);

        let route = {
            let steered = zombie.get_mob_entity().steered();
            let navigator = steered.navigator();
            navigator.get_path().map(|path| {
                let detours = (0..path.get_node_count())
                    .filter_map(|i| path.get_node(i))
                    .any(|node| node.pos.0.z.abs() >= 4);
                (detours, path.get_end_node().map(|node| node.pos.0))
            })
        };
        world.level.shutdown().await;
        let (detours, end) = route.ok_or("start did not install the route")?;
        let end = end.ok_or("empty route")?;

        assert!(detours, "the route must go around the wall");
        assert_eq!((end.x, end.z), (6, 0), "the route must end at the target");
        Ok(())
    }

    #[tokio::test]
    async fn failed_repath_adds_vanilla_fifteen_tick_delay() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        stone_floor(&world);
        let (zombie, _pig) = spawn_pair(&world, 6.5);
        // A walker in mid-air cannot plan (`canUpdatePath`), so `createPath` returns null.
        zombie
            .get_entity()
            .on_ground
            .store(false, Ordering::Relaxed);

        let mut goal = MeleeAttackGoal::new(1.0, true);
        assert!(
            !goal.can_start(&*zombie),
            "no route and out of reach must not start the goal"
        );
        goal.tick(&*zombie);
        world.level.shutdown().await;

        // 4 + nextInt(7) is at most 10, so only the failure bonus gets it past 18.
        assert!(
            goal.update_countdown_ticks >= 19,
            "expected the +15 repath delay, got {}",
            goal.update_countdown_ticks
        );
        Ok(())
    }
}
