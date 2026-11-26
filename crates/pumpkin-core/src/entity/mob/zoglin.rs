use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pumpkin_data::BlockState;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::position::BlockPos;

use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainState, BrainTick,
    activity::ActivityData,
    behavior::{
        BehaviorControl, DoNothing, LookAtTargetSink, MoveToTargetSink, Timed, melee_attack,
        random_stroll, run_one, set_entity_look_target_sometimes,
        set_walk_target_from_attack_target_if_target_out_of_reach,
        set_walk_target_from_look_target, start_attacking, stop_attacking_if_target_invalid, utils,
    },
    memory::{PackedMemories, types},
    sensing::{SensorType, is_entity_attackable},
};
use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity, MobMetadata, hoglin::hurt_and_throw_target},
};
use crate::world::World;

const ATTACK_INTERVAL: i64 = 40;
const BABY_ATTACK_INTERVAL: i64 = 15;
const ATTACK_DURATION: i64 = 200;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.4;

pub struct ZoglinEntity {
    pub mob_entity: MobEntity,
    pub is_baby: AtomicBool,
}

impl ZoglinEntity {
    pub const XP_REWARD: u32 = 5;
    const BABY_ATTACK_DAMAGE: f64 = 0.5;

    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        {
            let mut attributes = mob_entity
                .living_entity
                .attributes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(health) = attributes.get_mut(&Attributes::MAX_HEALTH.id) {
                health.base_value = 40.0;
                health.dirty.store(true, Ordering::Relaxed);
            }
            if let Some(speed) = attributes.get_mut(&Attributes::MOVEMENT_SPEED.id) {
                speed.base_value = 0.3;
                speed.dirty.store(true, Ordering::Relaxed);
            }
            if let Some(knockback_res) = attributes.get_mut(&Attributes::KNOCKBACK_RESISTANCE.id) {
                knockback_res.base_value = 0.6;
                knockback_res.dirty.store(true, Ordering::Relaxed);
            }
            if let Some(attack_kb) = attributes.get_mut(&Attributes::ATTACK_KNOCKBACK.id) {
                attack_kb.base_value = 1.0;
                attack_kb.dirty.store(true, Ordering::Relaxed);
            }
            if let Some(damage) = attributes.get_mut(&Attributes::ATTACK_DAMAGE.id) {
                damage.base_value = 6.0;
                damage.dirty.store(true, Ordering::Relaxed);
            }
        }
        mob_entity.living_entity.health.store(40.0);

        Arc::new(Self {
            mob_entity,
            is_baby: AtomicBool::new(false),
        })
    }

    #[must_use]
    pub fn is_baby(&self) -> bool {
        self.is_baby.load(Ordering::Relaxed)
    }

    /// Vanilla `Zoglin.setBaby`; growing up keeps the lowered attack damage, as in vanilla.
    pub fn set_baby(&self, baby: bool) {
        self.mob_entity
            .set_baby_flag(&self.is_baby, tracked_data::zoglin::DATA_BABY_ID, baby);
        if baby {
            self.mob_entity
                .living_entity
                .set_attribute_base(&Attributes::ATTACK_DAMAGE, Self::BABY_ATTACK_DAMAGE);
        }
    }
}

fn is_baby(tick: &BrainTick<'_>) -> bool {
    tick.mob.get_mob_entity().living_entity.is_baby()
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
            ],
        ),
        ActivityData::with_priority_start(
            Activity::Idle,
            10,
            vec![
                Box::new(start_attacking::create(
                    |_| true,
                    find_nearest_valid_attack_target,
                )),
                Box::new(set_entity_look_target_sometimes::create(8.0, 30, 60)),
                idle_movement(),
            ],
        ),
        ActivityData::with_gate_memory(
            Activity::Fight,
            10,
            vec![
                Box::new(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
                Box::new(melee_attack::create_if(
                    |tick| !is_baby(tick),
                    ATTACK_INTERVAL,
                )),
                Box::new(melee_attack::create_if(is_baby, BABY_ATTACK_INTERVAL)),
                Box::new(stop_attacking_if_target_invalid::create()),
            ],
            types::ATTACK_TARGET.id(),
        ),
    ]
}

fn idle_movement() -> Box<dyn BehaviorControl> {
    Box::new(run_one(vec![
        (
            Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
            2,
        ),
        (
            Box::new(set_walk_target_from_look_target::create(
                SPEED_MULTIPLIER_WHEN_IDLING,
                3,
            )),
            2,
        ),
        (Box::new(DoNothing::new(30, 60)), 1),
    ]))
}

/// Vanilla `Zoglin.findNearestValidAttackTarget`: anything attackable but zoglins and creepers.
fn find_nearest_valid_attack_target(tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
    let ctx = tick.visibility();
    tick.state
        .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)?
        .find_closest(&ctx, |entity| {
            let entity_type = entity.get_entity().entity_type;
            entity_type != &EntityType::ZOGLIN
                && entity_type != &EntityType::CREEPER
                && is_entity_attackable(&ctx, entity.as_ref())
        })
}

/// Vanilla `Zoglin.setAttackTarget`.
fn set_attack_target(brain: &mut BrainState, target: Arc<dyn EntityBase>) {
    brain.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
    brain.set_with_expiry(types::ATTACK_TARGET, target, ATTACK_DURATION);
}

impl Mob for ZoglinEntity {
    fn is_baby_mob(&self) -> bool {
        Self::is_baby(self)
    }

    /// Vanilla `blockedByItem`: adults still toss the shield holder.
    fn blocked_by_item(&self, defender: &dyn EntityBase, _fully_blocked: bool) -> bool {
        let living = &self.get_mob_entity().living_entity;
        if !living.is_baby() {
            crate::entity::mob::hoglin::throw_target(living, defender);
        }
        true
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        BrainProvider {
            memory_types: &[],
            sensor_types: &[
                SensorType::NearestLivingEntities,
                SensorType::NearestPlayers,
            ],
            activities,
        }
        .make_brain(self, packed)
    }

    /// Vanilla `Zoglin.updateActivity`: growls on entering a fight.
    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        let before = tick.state.get_active_non_core_activity();
        tick.state
            .set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
        let after = tick.state.get_active_non_core_activity();
        if after == Some(Activity::Fight) && before != Some(Activity::Fight) {
            self.make_sound(Sound::EntityZoglinAngry);
        }
        self.mob_entity
            .set_attacking(tick.state.has_memory_value(types::ATTACK_TARGET.id()));
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        // Vanilla `getTarget` reads the brain's attack target.
        let target = self
            .mob_entity
            .with_brain(self, |brain| brain.get(types::ATTACK_TARGET).cloned());
        self.mob_entity.set_target(target);
    }

    /// Vanilla `Zoglin.doHurtTarget`.
    fn do_hurt_target(&self, target: &dyn EntityBase) -> bool {
        if target.get_living_entity().is_none() {
            return false;
        }
        let entity = self.get_entity();
        entity.world.load().send_entity_status(
            entity,
            EntityStatus::StartAttacking,
            ActorEventID::StartAttacking,
        );
        self.make_sound(Sound::EntityZoglinAttack);
        hurt_and_throw_target(self, target)
    }

    /// Vanilla `Zoglin.hurtServer`: turns on an attacker that isn't much further than its target.
    fn on_damage_with_context(
        &self,
        _damage_type: DamageType,
        _source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        let Some(attacker) = cause.and_then(|cause| {
            self.get_entity()
                .world
                .load()
                .get_entity_by_id(cause.get_entity().entity_id)
        }) else {
            return true;
        };
        if attacker.get_living_entity().is_none() || !self.can_attack(attacker.as_ref()) {
            return true;
        }
        self.mob_entity.post_to_brain(Box::new(move |tick| {
            if !utils::is_other_target_much_further_away_than_current_attack_target(
                tick.state,
                tick.mob,
                attacker.as_ref(),
                4.0,
            ) {
                set_attack_target(tick.state, attacker);
            }
        }));
        true
    }

    /// Vanilla `getAmbientSound`: angry while it has an attack target.
    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(
            if self.mob_entity.with_brain(self, |brain| {
                brain.has_memory_value(types::ATTACK_TARGET.id())
            }) {
                Sound::EntityZoglinAngry
            } else {
                Sound::EntityZoglinAmbient
            },
        )
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntityZoglinStep, 0.15, 1.0);
    }

    fn mob_metadata(&self) -> Option<MobMetadata> {
        self.is_baby().then(|| MobMetadata::new().baby(true))
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn finalize_spawn(
        &self,
        _world: &Arc<World>,
        _reason: crate::entity::mob::spawn::EntitySpawnReason,
        group_data: Option<crate::entity::mob::spawn::SpawnGroupData>,
    ) -> Option<crate::entity::mob::spawn::SpawnGroupData> {
        if rand::random::<f32>() < 0.2 {
            self.set_baby(true);
        }
        self.mob_entity.finalize_spawn_base();
        group_data
    }

    fn spawn_as_baby(&self) -> bool {
        self.set_baby(true);
        true
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("IsBaby", self.is_baby());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.set_baby(nbt.get_bool("IsBaby").unwrap_or(false));
    }
}
