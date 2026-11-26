use std::sync::atomic::{AtomicI32, Ordering::Relaxed};
use std::sync::{Arc, Weak};

use pumpkin_data::BlockState;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;

use crate::entity::ageable::AgeableMob;
use crate::entity::ai::behavior::hunting::{self, Hunt};
use crate::entity::ai::goal::look_at_entity::LookAtEntityGoal as LookGoal;
use crate::entity::ai::target_match::TargetMatch;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{melee_attack::MeleeAttackGoal, swim::SwimGoal, wander_around::WanderAroundGoal},
    mob::{
        Mob, MobEntity,
        patrol::{LongDistancePatrolGoal, PatrolData, PatrollingMonster},
        raider::{
            self, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal, Raider, RaiderCelebrationGoal,
            RaiderData, RaiderMoveThroughVillageGoal,
        },
    },
};

const RAVAGER_HUNTS: &[Hunt] = &[
    Hunt::new(3, TargetMatch::Type(&EntityType::PLAYER), true),
    Hunt::new(4, TargetMatch::VILLAGER_LIKE, true).only_mobs(is_adult),
    Hunt::new(4, TargetMatch::Type(&EntityType::IRON_GOLEM), true),
];

fn is_adult(mob: &dyn Mob) -> bool {
    !mob.as_ageable().is_some_and(AgeableMob::is_baby)
}

/// Illagers are spared by the roar's damage (vanilla `AbstractIllager`).
const ILLAGERS: [&EntityType; 4] = [
    &EntityType::EVOKER,
    &EntityType::ILLUSIONER,
    &EntityType::PILLAGER,
    &EntityType::VINDICATOR,
];

pub struct RavagerEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    attack_tick: AtomicI32,
    stunned_tick: AtomicI32,
    roar_tick: AtomicI32,
}

impl RavagerEntity {
    #[must_use]
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let ravager = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            attack_tick: AtomicI32::new(0),
            stunned_tick: AtomicI32::new(0),
            roar_tick: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(ravager);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(ObtainRaidLeaderBannerGoal));
            goal_selector.add_goal(3, Box::new(PathfindToRaidGoal::default()));
            goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.0, true)));
            goal_selector.add_goal(4, Box::new(LongDistancePatrolGoal::new(0.7, 0.595)));
            goal_selector.add_goal(4, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
            goal_selector.add_goal(5, Box::new(RaiderCelebrationGoal));
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::water_avoiding(0.4)));
            goal_selector.add_goal(
                6,
                Box::new(LookGoal::new(
                    mob_weak.clone(),
                    &EntityType::PLAYER,
                    6.0,
                    0.02,
                    false,
                )),
            );
            goal_selector.add_goal(
                10,
                Box::new(LookGoal::matching(
                    mob_weak,
                    TargetMatch::Mob,
                    8.0,
                    0.02,
                    false,
                )),
            );

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(2, raider::revenge_goal());
            hunting::apply(target_selector, &mob_arc.mob_entity, RAVAGER_HUNTS);
        };

        mob_arc
    }

    #[must_use]
    pub fn attack_tick(&self) -> i32 {
        self.attack_tick.load(Relaxed)
    }

    #[must_use]
    pub fn stunned_tick(&self) -> i32 {
        self.stunned_tick.load(Relaxed)
    }

    #[must_use]
    pub fn roar_tick(&self) -> i32 {
        self.roar_tick.load(Relaxed)
    }

    /// Vanilla `strongKnockback`.
    fn strong_knockback(&self, target: &dyn EntityBase) {
        let pos = self.get_entity().pos.load();
        let target_pos = target.get_entity().pos.load();
        let dx = target_pos.x - pos.x;
        let dz = target_pos.z - pos.z;
        let d = (dx * dx + dz * dz).max(0.001);
        target
            .get_entity()
            .push_by(Vector3::new(dx / d * 4.0, 0.2, dz / d * 4.0));
    }

    /// Vanilla `roar`: hurts and flings everything close by once the stun wears off.
    fn roar(&self) {
        let entity = self.get_entity();
        if !entity.is_alive() {
            return;
        }
        let world = entity.world.load();
        let griefing = world.level_info.load().game_rules.mob_griefing;
        let area = entity.bounding_box.load().expand(4.0, 4.0, 4.0);
        for target in world.get_all_at_box(&area) {
            let target_entity = target.get_entity();
            let Some(living) = target.get_living_entity() else {
                continue;
            };
            if target_entity.entity_type == &EntityType::RAVAGER
                || !living.is_alive()
                || (!griefing && target_entity.entity_type == &EntityType::ARMOR_STAND)
            {
                continue;
            }
            if !ILLAGERS.contains(&target_entity.entity_type) {
                target.damage_with_context(
                    target.as_ref(),
                    6.0,
                    DamageType::MOB_ATTACK,
                    None,
                    Some(self),
                    Some(self),
                );
            }
            if target.get_player().is_none() {
                self.strong_knockback(target.as_ref());
            }
        }
        entity.emit_own_game_event(self, GameEvent::EntityAction);
        world.send_entity_status(entity, EntityStatus::RavagerRoared, ActorEventID::None);
    }

    /// Vanilla: with `mobGriefing`, leaves in the way are torn down, else the ravager hops.
    fn break_leaves(&self) {
        let entity = self.get_entity();
        let world = entity.world.load();
        if !entity.horizontal_collision.load(Relaxed)
            || !world.level_info.load().game_rules.mob_griefing
        {
            return;
        }
        let area = entity.bounding_box.load().expand(0.2, 0.2, 0.2);
        let mut broke = false;
        for x in area.min.x.floor() as i32..=area.max.x.floor() as i32 {
            for y in area.min.y.floor() as i32..=area.max.y.floor() as i32 {
                for z in area.min.z.floor() as i32..=area.max.z.floor() as i32 {
                    let pos = BlockPos::new(x, y, z);
                    if world.get_block(&pos).has_tag(&tag::Block::MINECRAFT_LEAVES)
                        && world
                            .break_block(&pos, None, BlockFlags::NOTIFY_ALL)
                            .is_some()
                    {
                        broke = true;
                    }
                }
            }
        }
        if !broke && entity.on_ground.load(Relaxed) {
            self.mob_entity.living_entity.jump(self);
        }
    }
}

impl Mob for RavagerEntity {
    /// Vanilla `Ravager.updateControlFlags`: a raider rider (pillager, vindicator, evoker...)
    /// leaves the ravager's own goals on; any other mob controller switches them off.
    fn keeps_goals_under_controller(&self, controller: &dyn EntityBase) -> bool {
        crate::entity::mob::raider::is_raider(controller.get_entity().entity_type)
    }

    fn control_flags_include_target(&self) -> bool {
        true
    }
    fn mob_ai_step(&self) {
        let living = &self.mob_entity.living_entity;
        if !self.get_entity().is_alive() {
            return;
        }
        if self.is_mob_immobile() {
            living.set_attribute_base(&Attributes::MOVEMENT_SPEED, 0.0);
        } else {
            let goal = if self.mob_entity.get_target().is_some() {
                0.35
            } else {
                0.3
            };
            let current = living.get_attribute_base(&Attributes::MOVEMENT_SPEED);
            living.set_attribute_base(
                &Attributes::MOVEMENT_SPEED,
                current + 0.1 * (goal - current),
            );
        }

        self.break_leaves();

        if self.roar_tick.load(Relaxed) > 0 && self.roar_tick.fetch_sub(1, Relaxed) - 1 == 10 {
            self.roar();
        }
        if self.attack_tick.load(Relaxed) > 0 {
            self.attack_tick.fetch_sub(1, Relaxed);
        }
        if self.stunned_tick.load(Relaxed) > 0 && self.stunned_tick.fetch_sub(1, Relaxed) == 1 {
            self.play_entity_sound(Sound::EntityRavagerRoar, 1.0, 1.0);
            self.roar_tick.store(20, Relaxed);
        }
    }

    fn is_mob_immobile(&self) -> bool {
        self.mob_entity.living_entity.is_dead_or_dying()
            || self.attack_tick.load(Relaxed) > 0
            || self.stunned_tick.load(Relaxed) > 0
            || self.roar_tick.load(Relaxed) > 0
    }

    fn has_line_of_sight(&self, target: &Entity) -> bool {
        if self.stunned_tick.load(Relaxed) > 0 || self.roar_tick.load(Relaxed) > 0 {
            return false;
        }
        let mob_entity = self.get_mob_entity();
        mob_entity
            .sensing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .has_line_of_sight(&mob_entity.living_entity.entity, target)
    }

    fn get_max_head_rotation(&self) -> f32 {
        45.0
    }

    /// Vanilla `blockedByItem`: a shield block may stun the ravager, else it rams through.
    fn blocked_by_item(&self, defender: &dyn EntityBase, _fully_blocked: bool) -> bool {
        if self.roar_tick.load(Relaxed) == 0 {
            if rand::random::<f64>() < 0.5 {
                self.stunned_tick.store(40, Relaxed);
                self.play_entity_sound(Sound::EntityRavagerStunned, 1.0, 1.0);
                let entity = self.get_entity();
                entity.world.load().send_entity_status(
                    entity,
                    EntityStatus::RavagerStunned,
                    ActorEventID::None,
                );
                defender.push(self);
            } else {
                self.strong_knockback(defender);
            }
            defender.get_entity().velocity_dirty.store(true, Relaxed);
        }
        true
    }

    fn do_hurt_target(&self, target: &dyn EntityBase) -> bool {
        self.attack_tick.store(10, Relaxed);
        let entity = self.get_entity();
        entity.world.load().send_entity_status(
            entity,
            EntityStatus::StartAttacking,
            ActorEventID::StartAttacking,
        );
        self.play_entity_sound(Sound::EntityRavagerAttack, 1.0, 1.0);
        self.mob_entity.try_attack(entity, target)
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntityRavagerStep, 0.15, 1.0);
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_attack_bounding_box(
        &self,
        horizontal_expansion: f64,
    ) -> pumpkin_util::math::boundingbox::BoundingBox {
        // Vanilla `Ravager.getAttackBoundingBox`
        super::attack_bounding_box(&self.mob_entity.living_entity.entity, horizontal_expansion)
            .expand(-0.05, 0.0, -0.05)
    }

    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
        nbt.put_int("AttackTick", self.attack_tick.load(Relaxed));
        nbt.put_int("StunTick", self.stunned_tick.load(Relaxed));
        nbt.put_int("RoarTick", self.roar_tick.load(Relaxed));
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
        self.attack_tick
            .store(nbt.get_int("AttackTick").unwrap_or(0), Relaxed);
        self.stunned_tick
            .store(nbt.get_int("StunTick").unwrap_or(0), Relaxed);
        self.roar_tick
            .store(nbt.get_int("RoarTick").unwrap_or(0), Relaxed);
    }
}

impl PatrollingMonster for RavagerEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }

    fn can_be_leader(&self) -> bool {
        false
    }
}

impl Raider for RavagerEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityRavagerCelebrate
    }
}
