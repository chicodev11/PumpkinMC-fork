use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::BlockState;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use super::hoglin_ai::{self, HoglinBrainEvent};
use crate::enchantment::EnchantmentHelper;
use crate::entity::ai::brain::{Brain, BrainTick, memory::PackedMemories, memory::types};
use crate::entity::living::LivingEntity;
use crate::entity::mob::conversion::{ConversionParams, convert_to};
use crate::entity::mob::spawn::SpawnGroupData;
use crate::entity::passive::animal::Animal;
use crate::entity::player::Player;
use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    mob::{Mob, MobEntity},
};
use crate::world::World;

/// Vanilla `Hoglin.finishConversion` gives the zoglin 10 seconds of nausea.
const CONVERSION_NAUSEA_DURATION: i32 = 200;

pub struct HoglinEntity {
    pub mob_entity: MobEntity,
    pub immune_to_zombification: AtomicBool,
    pub time_in_overworld: AtomicI32,
    pub cannot_be_hunted: AtomicBool,
    pub ageable_data: AgeableData,
    /// Copy of the `NEAREST_REPELLENT` memory for `getWalkTargetValue`, which runs inside
    /// the brain tick and so can't lock the brain.
    pub nearest_repellent: AtomicCell<Option<BlockPos>>,
}

impl HoglinEntity {
    pub const CONVERSION_TIME: i32 = 300;
    pub const XP_REWARD: u32 = 5;
    pub const BABY_XP_REWARD: u32 = 3;

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
            immune_to_zombification: AtomicBool::new(false),
            time_in_overworld: AtomicI32::new(0),
            cannot_be_hunted: AtomicBool::new(false),
            ageable_data: AgeableData::default(),
            nearest_repellent: AtomicCell::new(None),
        })
    }

    /// Posts a brain change to this hoglin's brain inbox; safe from any mob's tick.
    pub fn queue_brain_event(&self, event: HoglinBrainEvent) {
        self.mob_entity.post_to_brain(Box::new(move |tick| {
            if let Some(hoglin) = tick.mob.cast_any().downcast_ref::<Self>() {
                hoglin_ai::apply_event(hoglin, tick.state, tick.world, event);
            }
        }));
    }

    /// Vanilla `canBeHunted`: piglins hunt adults that haven't been marked off limits.
    #[must_use]
    pub fn can_be_hunted(&self) -> bool {
        !self.is_baby() && !self.cannot_be_hunted.load(Ordering::Relaxed)
    }

    pub fn is_immune_to_zombification(&self) -> bool {
        self.immune_to_zombification.load(Ordering::Relaxed)
    }

    pub fn set_immune_to_zombification(&self, immune: bool) {
        self.immune_to_zombification
            .store(immune, Ordering::Relaxed);
        self.mob_entity.living_entity.entity.set_synced_data(
            pumpkin_data::tracked_data::hoglin::DATA_IMMUNE_TO_ZOMBIFICATION,
            immune,
        );
    }

    pub fn is_converting(&self, world: &World) -> bool {
        !self.is_immune_to_zombification()
            && !self.mob_entity.is_no_ai()
            && world.dimension.piglins_zombify
    }

    /// Vanilla `finishConversion`.
    fn finish_conversion(&self) {
        convert_to(
            self,
            &EntityType::ZOGLIN,
            ConversionParams::single(true, false),
            |zoglin| {
                zoglin.get_mob_entity().living_entity.add_effect(Effect {
                    effect_type: &StatusEffect::NAUSEA,
                    duration: CONVERSION_NAUSEA_DURATION,
                    amplifier: 0,
                    ambient: false,
                    show_particles: true,
                    show_icon: true,
                    blend: false,
                });
            },
        );
    }
}

impl AgeableMob for HoglinEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    fn age_boundary_reached(&self) {
        let damage = if self.is_baby() { 0.5 } else { 6.0 };
        self.mob_entity
            .living_entity
            .set_attribute_base(&Attributes::ATTACK_DAMAGE, damage);
    }
}

impl Animal for HoglinEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_HOGLIN_FOOD)
    }

    fn get_breed_offspring(&self, _partner: &dyn EntityBase) -> Arc<dyn EntityBase> {
        let entity = self.get_entity();
        let baby = Self::new(Entity::new(
            entity.world.load_full(),
            entity.pos.load(),
            entity.entity_type,
        ));
        baby.mob_entity
            .persistence_required
            .store(true, Ordering::Relaxed);
        baby
    }

    fn can_fall_in_love(&self) -> bool {
        !self
            .mob_entity
            .with_brain(self, |brain| hoglin_ai::is_pacified(brain))
            && !self.mob_entity.is_in_love()
    }
}

impl Mob for HoglinEntity {
    /// Vanilla `blockedByItem`: adults still toss the shield holder.
    fn blocked_by_item(&self, defender: &dyn EntityBase, _fully_blocked: bool) -> bool {
        let living = &self.get_mob_entity().living_entity;
        if !living.is_baby() {
            throw_target(living, defender);
        }
        true
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        hoglin_ai::make_brain(self, packed)
    }

    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        hoglin_ai::update_activity(self, tick.state);
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        let world = self.get_entity().world.load_full();
        self.mob_entity.tick_brain(self);
        let target = self
            .mob_entity
            .with_brain(self, |brain| brain.get(types::ATTACK_TARGET).cloned());
        // `getTarget` reads the brain's attack target.
        self.mob_entity.set_target(target);

        if self.is_converting(&world) {
            let time = self.time_in_overworld.fetch_add(1, Ordering::Relaxed) + 1;
            if time > Self::CONVERSION_TIME {
                self.make_sound(Sound::EntityHoglinConvertedToZombified);
                self.finish_conversion();
            }
        } else {
            self.time_in_overworld.store(0, Ordering::Relaxed);
        }
    }

    fn do_hurt_target(&self, target: &dyn EntityBase) -> bool {
        let entity = self.get_entity();
        entity.world.load().send_entity_status(
            entity,
            EntityStatus::StartAttacking,
            ActorEventID::StartAttacking,
        );
        self.make_sound(Sound::EntityHoglinAttack);
        if let Some(target) = entity
            .world
            .load()
            .get_entity_by_id(target.get_entity().entity_id)
        {
            self.queue_brain_event(HoglinBrainEvent::HitTarget(target));
        }
        hurt_and_throw_target(self, target)
    }

    fn on_damage_with_context(
        &self,
        _damage_type: DamageType,
        _source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        if let Some(attacker) = cause
            .and_then(|cause| {
                self.get_entity()
                    .world
                    .load()
                    .get_entity_by_id(cause.get_entity().entity_id)
            })
            .filter(|attacker| attacker.get_living_entity().is_some())
        {
            self.queue_brain_event(HoglinBrainEvent::HurtBy(attacker));
        }
        true
    }

    fn get_ambient_sound(&self) -> Option<Sound> {
        self.mob_entity.with_brain(self, |brain| {
            hoglin_ai::sound_for_current_activity(self, brain)
        })
    }

    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        if self
            .nearest_repellent
            .load()
            .is_some_and(|repellent| hoglin_ai::is_near_repellent(repellent, pos))
        {
            return -1.0;
        }
        let world = self.get_entity().world.load();
        if world.get_block(&pos.down()) == &pumpkin_data::Block::CRIMSON_NYLIUM {
            10.0
        } else {
            0.0
        }
    }

    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        true
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntityHoglinStep, 0.15, 1.0);
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let consumed = self.animal_interact(player, item_stack);
        if consumed {
            self.mob_entity
                .persistence_required
                .store(true, Ordering::Relaxed);
        }
        consumed
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        _reason: crate::entity::mob::spawn::EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if self.get_random().random::<f32>() < 0.2 {
            self.set_baby(true);
        }
        self.default_finalize_spawn(world, group_data)
    }

    fn should_drop_experience(&self) -> bool {
        true
    }

    fn get_base_experience_reward(&self) -> u32 {
        if self.is_baby() {
            Self::BABY_XP_REWARD
        } else {
            Self::XP_REWARD
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if self.is_immune_to_zombification() {
            nbt.put_bool("IsImmuneToZombification", true);
        }
        let time = self.time_in_overworld.load(Ordering::Relaxed);
        if time > 0 {
            nbt.put_int("TimeInOverworld", time);
        }
        if self.cannot_be_hunted.load(Ordering::Relaxed) {
            nbt.put_bool("CannotBeHunted", true);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(immune) = nbt.get_bool("IsImmuneToZombification") {
            self.set_immune_to_zombification(immune);
        }
        if let Some(time) = nbt.get_int("TimeInOverworld") {
            self.time_in_overworld.store(time, Ordering::Relaxed);
        }
        if let Some(cannot_hunt) = nbt.get_bool("CannotBeHunted") {
            self.cannot_be_hunted.store(cannot_hunt, Ordering::Relaxed);
        }
    }
}

/// Vanilla `HoglinBase.hurtAndThrowTarget`, shared with zoglins.
pub fn hurt_and_throw_target(body: &dyn Mob, target: &dyn EntityBase) -> bool {
    let living = &body.get_mob_entity().living_entity;
    let base_damage = living.get_attribute_value(&Attributes::ATTACK_DAMAGE) as f32;
    let damage = if !living.is_baby() && base_damage as i32 > 0 {
        base_damage / 2.0 + body.get_random().random_range(0..base_damage as i32) as f32
    } else {
        base_damage
    };
    let entity = body.get_entity();
    let hurt = target.damage_with_context(
        target,
        damage,
        DamageType::MOB_ATTACK,
        None,
        Some(entity),
        Some(entity),
    );
    if hurt {
        EnchantmentHelper::on_post_attack(body, target.get_entity(), &living.held_item(body));
        if !living.is_baby() {
            throw_target(living, target);
        }
    }
    hurt
}

/// Vanilla `HoglinBase.throwTarget`, shared with zoglins.
pub fn throw_target(body: &LivingEntity, target: &dyn EntityBase) {
    let Some(target_living) = target.get_living_entity() else {
        return;
    };
    let power = body.get_attribute_value(&Attributes::ATTACK_KNOCKBACK)
        - target_living.get_attribute_value(&Attributes::KNOCKBACK_RESISTANCE);
    if power <= 0.0 {
        return;
    }
    let body_pos = body.entity.pos.load();
    let target_pos = target.get_entity().pos.load();
    let mut rng = rand::rng();
    // Vanilla passes these "degrees" straight to `Vec3.yRot`, which takes radians.
    let y_rot_offset = (rng.random_range(0..21) - 10) as f32;
    let horizontal = power * f64::from(rng.random::<f32>() * 0.5 + 0.2);
    let direction = Vector3::new(target_pos.x - body_pos.x, 0.0, target_pos.z - body_pos.z)
        .normalize()
        * horizontal;
    let (sin, cos) = f64::from(y_rot_offset).sin_cos();
    let push_x = direction.x * cos + direction.z * sin;
    let push_z = direction.z * cos - direction.x * sin;
    let up = power * f64::from(rng.random::<f32>()) * 0.5;
    target
        .get_entity()
        .push_by(Vector3::new(push_x, up, push_z));
}
