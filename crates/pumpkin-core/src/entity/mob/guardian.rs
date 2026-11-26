//! Vanilla `Guardian`, shared with [`super::elder_guardian`].

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering::Relaxed};
use std::sync::{Arc, Weak};

use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data::guardian::{DATA_ID_ATTACK_TARGET, DATA_ID_MOVING};
use pumpkin_data::{damage::DamageType, sound::Sound};
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::Difficulty;
use pumpkin_util::math::position::BlockPos;

use crate::entity::ai::behavior::hunting::{self, Hunt};
use crate::entity::ai::control::guardian_move_control::GuardianMoveControl;
use crate::entity::ai::goal::{Controls, Goal};
use crate::entity::ai::pathfinder::{Navigator, node::PathType};
use crate::entity::ai::target_match::TargetMatch;
use crate::entity::custom_sound::CustomSound;
use crate::entity::living::LivingEntity;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal,
        move_towards_restriction::MoveTowardsRestrictionGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
};
use crate::world::World;

const GUARDIAN_PREY: TargetMatch = TargetMatch::AnyOf(&[
    &EntityType::PLAYER,
    &EntityType::SQUID,
    &EntityType::GLOW_SQUID,
    &EntityType::AXOLOTL,
]);

/// Vanilla `GuardianAttackSelector`: nothing within 3 blocks.
fn outside_melee_range(hunter: &dyn Mob, target: &LivingEntity, _world: &World) -> bool {
    let hunter_pos = hunter.get_mob_entity().living_entity.entity.pos.load();
    target
        .entity
        .pos
        .load()
        .squared_distance_to_vec(&hunter_pos)
        > 9.0
}

pub const GUARDIAN_HUNTS: &[Hunt] =
    &[Hunt::new(1, GUARDIAN_PREY, true).matching(outside_melee_range)];

/// State shared by guardians and elder guardians, and with their goals and move control.
pub struct GuardianCore {
    pub mob_entity: MobEntity,
    elder: bool,
    moving: Arc<AtomicBool>,
    synced_moving: AtomicBool,
    active_attack_target: Arc<AtomicI32>,
    stroll_trigger: Arc<AtomicBool>,
}

impl GuardianCore {
    #[must_use]
    pub fn new(entity: Entity, elder: bool) -> Self {
        let moving = Arc::new(AtomicBool::new(false));
        let mut mob_entity = MobEntity::new(entity);
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(GuardianMoveControl::new(moving.clone()));
        let navigator = mob_entity
            .navigator
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *navigator = Navigator::water_bound(false);
        navigator.set_pathfinding_malus(PathType::Water, 0.0);
        Self {
            mob_entity,
            elder,
            moving,
            synced_moving: AtomicBool::new(false),
            active_attack_target: Arc::new(AtomicI32::new(0)),
            stroll_trigger: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Vanilla `Guardian.registerGoals`.
    pub fn register_goals(&self, mob_weak: Weak<dyn Mob>) {
        let goal_selector = &self.mob_entity.goals_selector;
        goal_selector.add_goal(
            4,
            Box::new(GuardianAttackGoal {
                elder: self.elder,
                attack_time: 0,
                active_attack_target: self.active_attack_target.clone(),
                stroll_trigger: self.stroll_trigger.clone(),
            }),
        );
        goal_selector.add_goal(
            5,
            Box::new(
                MoveTowardsRestrictionGoal::new(1.0).with_controls(Controls::MOVE | Controls::LOOK),
            ),
        );
        goal_selector.add_goal(
            7,
            Box::new(GuardianStrollGoal {
                inner: WanderAroundGoal::with_interval(1.0, 80),
                trigger: self.stroll_trigger.clone(),
            }),
        );
        goal_selector.add_goal(
            8,
            LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
        );
        goal_selector.add_goal(
            8,
            Box::new(LookAtEntityGoal::matching(
                mob_weak,
                TargetMatch::AnyOf(&[&EntityType::GUARDIAN, &EntityType::ELDER_GUARDIAN]),
                12.0,
                0.01,
                false,
            )),
        );
        goal_selector.add_goal(9, Box::new(RandomLookAroundGoal::default()));

        hunting::apply(
            &self.mob_entity.target_selector,
            &self.mob_entity,
            GUARDIAN_HUNTS,
        );
    }

    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.moving.load(Relaxed)
    }

    #[must_use]
    pub fn active_attack_target(&self) -> i32 {
        self.active_attack_target.load(Relaxed)
    }

    #[must_use]
    pub fn has_active_attack_target(&self) -> bool {
        self.active_attack_target() != 0
    }

    /// Vanilla `getAttackDuration`.
    #[must_use]
    pub const fn attack_duration(&self) -> i32 {
        if self.elder { 60 } else { 80 }
    }

    /// Vanilla `Guardian.aiStep`, the server part run before `Monster.aiStep`.
    pub fn ai_step(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        if !entity.is_alive() {
            return;
        }
        let moving = self.is_moving();
        if self.synced_moving.swap(moving, Relaxed) != moving {
            entity.set_synced_data(DATA_ID_MOVING, moving);
        }

        if entity.is_in_water() {
            entity.air_supply.store(300, Relaxed);
        } else if entity.on_ground.load(Relaxed) {
            let flop_x = f64::from((rand::random::<f32>() * 2.0 - 1.0) * 0.4);
            let flop_z = f64::from((rand::random::<f32>() * 2.0 - 1.0) * 0.4);
            entity
                .velocity
                .store(entity.velocity.load().add_raw(flop_x, 0.5, flop_z));
            entity.yaw.store(rand::random::<f32>() * 360.0);
            entity.on_ground.store(false, Relaxed);
            entity.velocity_dirty.store(true, Relaxed);
        }
        if self.has_active_attack_target() {
            entity.yaw.store(entity.head_yaw.load());
        }
    }

    /// Vanilla `Guardian.hurtServer`, the part before `super`: spikes hurt attackers while
    /// the guardian sits still.
    pub fn retaliate(
        &self,
        caller: &dyn EntityBase,
        damage_type: DamageType,
        source: Option<&dyn EntityBase>,
    ) {
        if !self.is_moving()
            && !damage_type.has_tag(&tag::DamageType::MINECRAFT_AVOIDS_GUARDIAN_THORNS)
            && damage_type != DamageType::THORNS
            && let Some(attacker) = source
            && attacker.get_living_entity().is_some()
        {
            attacker.damage_with_context(
                attacker,
                2.0,
                DamageType::THORNS,
                None,
                Some(caller),
                Some(caller),
            );
        }
        self.stroll_trigger.store(true, Relaxed);
    }

    /// Vanilla `Guardian.travelInWater`.
    pub fn travel_in_water(&self, caller: &dyn EntityBase) {
        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        entity.update_velocity_from_input(living.movement_input.load(), 0.1);
        living.make_move(caller);
        let mut velocity = entity.velocity.load() * 0.9;
        if !self.is_moving() && self.mob_entity.get_target().is_none() {
            velocity.y -= 0.005;
        }
        entity.velocity.store(velocity);
    }

    /// Vanilla `Guardian.getWalkTargetValue`: water is home, lit water better still.
    #[must_use]
    pub fn walk_target_value(&self, pos: &BlockPos) -> Option<f32> {
        let world = self.mob_entity.living_entity.entity.world.load();
        let (fluid, _) = world.get_fluid_and_fluid_state(pos);
        fluid
            .has_tag(&tag::Fluid::MINECRAFT_WATER)
            .then(|| 10.0 + world.get_pathfinding_cost_from_light_levels(pos))
    }
}

/// Vanilla `RandomStrollGoal(1.0, 80)` with `MOVE | LOOK`, re-armed by `trigger()`.
struct GuardianStrollGoal {
    inner: WanderAroundGoal,
    trigger: Arc<AtomicBool>,
}

impl Goal for GuardianStrollGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if self.trigger.swap(false, Relaxed) {
            self.inner.trigger();
        }
        self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

/// Vanilla `Guardian.GuardianAttackGoal`: lock on, charge the beam, then zap.
struct GuardianAttackGoal {
    elder: bool,
    attack_time: i32,
    active_attack_target: Arc<AtomicI32>,
    stroll_trigger: Arc<AtomicBool>,
}

impl GuardianAttackGoal {
    fn set_active_attack_target(&self, mob: &dyn Mob, id: i32) {
        self.active_attack_target.store(id, Relaxed);
        mob.get_entity().set_synced_data(DATA_ID_ATTACK_TARGET, id);
    }

    fn look_at(mob: &dyn Mob, target: &Arc<dyn EntityBase>) {
        mob.get_mob_entity()
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(target, 90.0, 90.0);
    }
}

impl Goal for GuardianAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .get_target()
            .is_some_and(|target| target.get_entity().is_alive())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        if !self.can_start(mob) {
            return false;
        }
        self.elder
            || mob.get_mob_entity().get_target().is_some_and(|target| {
                target
                    .get_entity()
                    .pos
                    .load()
                    .squared_distance_to_vec(&mob.get_entity().pos.load())
                    > 9.0
            })
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.attack_time = -10;
        mob.get_mob_entity().steered().navigator().stop();
        if let Some(target) = mob.get_mob_entity().get_target() {
            Self::look_at(mob, &target);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.set_active_attack_target(mob, 0);
        mob.get_mob_entity().set_target(None);
        self.stroll_trigger.store(true, Relaxed);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let Some(target) = mob_entity.get_target() else {
            return;
        };
        mob_entity.steered().navigator().stop();
        Self::look_at(mob, &target);
        if !mob.has_line_of_sight(target.get_entity()) {
            mob_entity.set_target(None);
            return;
        }
        self.attack_time += 1;
        let entity = mob.get_entity();
        if self.attack_time == 0 {
            self.set_active_attack_target(mob, target.get_entity().entity_id);
            if !entity.is_silent() {
                entity.world.load().send_entity_status(
                    entity,
                    EntityStatus::GuardianAttackSound,
                    ActorEventID::None,
                );
            }
        } else if self.attack_time >= if self.elder { 60 } else { 80 } {
            let mut damage = 1.0;
            if entity.world.load().level_info.load().difficulty == Difficulty::Hard {
                damage += 2.0;
            }
            if self.elder {
                damage += 2.0;
            }
            let Some(caller) = entity.world.load().get_entity_by_id(entity.entity_id) else {
                return;
            };
            target.damage_with_context(
                target.as_ref(),
                damage,
                DamageType::INDIRECT_MAGIC,
                None,
                Some(caller.as_ref()),
                Some(caller.as_ref()),
            );
            mob.do_hurt_target(target.as_ref());
            mob_entity.set_target(None);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

pub struct GuardianEntity {
    pub core: GuardianCore,
}

impl GuardianEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_arc = Arc::new(Self {
            core: GuardianCore::new(entity, false),
        });
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };
        mob_arc.core.register_goals(mob_weak);
        mob_arc
    }
}

impl CustomSound for GuardianEntity {
    fn hurt_sound(&self) -> Option<Sound> {
        Some(if self.get_entity().is_in_water() {
            Sound::EntityGuardianHurt
        } else {
            Sound::EntityGuardianHurtLand
        })
    }

    fn death_sound(&self) -> Option<Sound> {
        Some(if self.get_entity().is_in_water() {
            Sound::EntityGuardianDeath
        } else {
            Sound::EntityGuardianDeathLand
        })
    }
}

/// The [`Mob`] methods guardians and elder guardians share.
#[macro_export]
macro_rules! guardian_mob_methods {
    () => {
        fn get_mob_entity(&self) -> &$crate::entity::mob::MobEntity {
            &self.core.mob_entity
        }

        fn as_custom_sound(&self) -> Option<&dyn $crate::entity::custom_sound::CustomSound> {
            Some(self)
        }

        fn get_ambient_sound_interval(&self) -> i32 {
            160
        }

        fn get_max_look_pitch_change(&self) -> f32 {
            180.0
        }

        fn before_mob_ai_step(&self) {
            self.core.ai_step();
        }

        fn travel_in_water(
            &self,
            caller: &dyn $crate::entity::EntityBase,
            _gravity: f64,
            _falling: bool,
        ) {
            self.core.travel_in_water(caller);
        }

        fn get_walk_target_value(&self, pos: &pumpkin_util::math::position::BlockPos) -> f32 {
            self.core.walk_target_value(pos).unwrap_or_else(|| {
                -self
                    .get_entity()
                    .world
                    .load()
                    .get_pathfinding_cost_from_light_levels(pos)
            })
        }

        fn mob_hurt_server(
            &self,
            _amount: f32,
            damage_type: pumpkin_data::damage::DamageType,
            _position: Option<pumpkin_util::math::vector3::Vector3<f64>>,
            source: Option<&dyn $crate::entity::EntityBase>,
            _cause: Option<&dyn $crate::entity::EntityBase>,
        ) -> Option<bool> {
            self.core.retaliate(self, damage_type, source);
            None
        }
    };
}

impl Mob for GuardianEntity {
    crate::guardian_mob_methods!();

    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(if self.get_entity().is_in_water() {
            Sound::EntityGuardianAmbient
        } else {
            Sound::EntityGuardianAmbientLand
        })
    }
}
