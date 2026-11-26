use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{BlockDirection, BlockState};
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;

use crate::block::blocks::infested;
use crate::entity::ai::behavior::hunting::{self, Hunt};
use crate::entity::ai::goal::{Controls, Goal, to_goal_ticks};
use crate::entity::ai::target_match::TargetMatch;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal, melee_attack::MeleeAttackGoal,
        revenge::RevengeGoal, swim::SwimGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
};

const SILVERFISH_HUNTS: &[Hunt] = &[Hunt::new(2, TargetMatch::Type(&EntityType::PLAYER), true)];

/// Vanilla `Silverfish.SilverfishWakeUpFriendsGoal`: a hurt silverfish breaks open infested
/// blocks around it.
struct WakeUpFriendsGoal {
    look_for_friends: Arc<AtomicI32>,
}

impl Goal for WakeUpFriendsGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.look_for_friends.load(Ordering::Relaxed) > 0
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if self.look_for_friends.fetch_sub(1, Ordering::Relaxed) - 1 > 0 {
            return;
        }
        let entity = mob.get_entity();
        let world = entity.world.load();
        let griefing = world.level_info.load().game_rules.mob_griefing;
        let origin = entity.block_pos.load();
        let mut rng = mob.get_random();
        // 0, 1, -1, 2, -2, ...
        let next = |v: i32| i32::from(v <= 0) - v;
        let mut y = 0;
        while (-5..=5).contains(&y) {
            let mut x = 0;
            while (-10..=10).contains(&x) {
                let mut z = 0;
                while (-10..=10).contains(&z) {
                    let pos = origin.offset(pumpkin_util::math::vector3::Vector3::new(x, y, z));
                    let (block, state) = world.get_block_and_state_id(&pos);
                    if infested::is_infested(block) {
                        if griefing {
                            world.break_block(&pos, None, BlockFlags::NOTIFY_ALL);
                        } else if let Some(host) = infested::host_state_by_infested(block, state) {
                            world.set_block_state(&pos, host, BlockFlags::NOTIFY_ALL);
                        }
                        if rng.random::<bool>() {
                            return;
                        }
                    }
                    z = next(z);
                }
                x = next(x);
            }
            y = next(y);
        }
    }
}

/// Vanilla `Silverfish.SilverfishMergeWithStoneGoal`: an idle silverfish may burrow into a
/// neighbouring stone block, else it strolls.
struct MergeWithStoneGoal {
    stroll: WanderAroundGoal,
    selected_direction: BlockDirection,
    do_merge: bool,
}

impl MergeWithStoneGoal {
    fn merge_pos(mob: &dyn Mob, direction: BlockDirection) -> BlockPos {
        let pos = mob.get_entity().pos.load();
        BlockPos::floored(pos.x, pos.y + 0.5, pos.z).offset(direction.to_offset())
    }
}

impl Goal for MergeWithStoneGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let mob_entity = mob.get_mob_entity();
        if mob_entity.get_target().is_some() {
            return false;
        }
        if !mob_entity.steered().navigator().is_done() {
            return false;
        }
        let world = mob.get_entity().world.load();
        let mut rng = mob.get_random();
        if world.level_info.load().game_rules.mob_griefing
            && rng.random_range(0..to_goal_ticks(10)) == 0
        {
            self.selected_direction = BlockDirection::all()[rng.random_range(0..6)];
            let pos = Self::merge_pos(mob, self.selected_direction);
            if infested::is_compatible_host_block(world.get_block(&pos)) {
                self.do_merge = true;
                return true;
            }
        }
        self.do_merge = false;
        self.stroll.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        !self.do_merge && self.stroll.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        if !self.do_merge {
            self.stroll.start(mob);
            return;
        }
        let entity = mob.get_entity();
        let world = entity.world.load();
        let pos = Self::merge_pos(mob, self.selected_direction);
        let (block, state) = world.get_block_and_state_id(&pos);
        if let Some(infested) = infested::infested_state_by_host(block, state) {
            world.set_block_state(&pos, infested, BlockFlags::NOTIFY_ALL);
            world.send_entity_status(
                entity,
                EntityStatus::SilverfishMergeAnim,
                ActorEventID::None,
            );
            entity.remove();
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.stroll.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.stroll.tick(mob);
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

pub struct SilverfishEntity {
    pub entity: Arc<MobEntity>,
    look_for_friends: Arc<AtomicI32>,
}

impl SilverfishEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let look_for_friends = Arc::new(AtomicI32::new(0));
        let mob_arc = Arc::new(Self {
            entity: Arc::new(MobEntity::new(entity)),
            look_for_friends: look_for_friends.clone(),
        });

        let goal_selector = &mob_arc.entity.goals_selector;
        let target_selector = &mob_arc.entity.target_selector;

        goal_selector.add_goal(1, Box::new(SwimGoal::default()));
        goal_selector.add_goal(1, Box::new(ClimbOnTopOfPowderSnowGoal));
        goal_selector.add_goal(3, Box::new(WakeUpFriendsGoal { look_for_friends }));
        goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.0, false)));
        goal_selector.add_goal(
            5,
            Box::new(MergeWithStoneGoal {
                stroll: WanderAroundGoal::with_interval(1.0, 10),
                selected_direction: BlockDirection::Down,
                do_merge: false,
            }),
        );

        target_selector.add_goal(1, Box::new(RevengeGoal::new(true).alerting_others()));
        hunting::apply(target_selector, &mob_arc.entity, SILVERFISH_HUNTS);

        mob_arc
    }
}

impl Mob for SilverfishEntity {
    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntitySilverfishStep, 0.15, 1.0);
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = &self.entity.living_entity.entity;
        if !entity.is_alive() {
            return;
        }

        let yaw = entity.yaw.load();
        entity.body_yaw.store(yaw);
        entity.head_yaw.store(yaw);
    }

    /// Vanilla `Silverfish.hurtServer`: an attacker, or a hit that always counts, alerts
    /// the silverfish hiding nearby.
    fn mob_hurt_server(
        &self,
        _amount: f32,
        damage_type: DamageType,
        _position: Option<pumpkin_util::math::vector3::Vector3<f64>>,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> Option<bool> {
        let entity = self.get_entity();
        if !entity.is_invulnerable_to(&damage_type, cause)
            && (cause.or(source).is_some()
                || damage_type.has_tag(&tag::DamageType::MINECRAFT_ALWAYS_TRIGGERS_SILVERFISH))
        {
            let _ = self.look_for_friends.compare_exchange(
                0,
                to_goal_ticks(20),
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
        None
    }

    /// Vanilla `Silverfish.getWalkTargetValue`: it likes standing on stone it can hide in.
    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        let world = self.get_entity().world.load();
        if infested::is_compatible_host_block(world.get_block(&pos.down())) {
            10.0
        } else {
            -world.get_pathfinding_cost_from_light_levels(pos)
        }
    }
}
