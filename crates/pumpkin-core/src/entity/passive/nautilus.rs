use crossbeam::atomic::AtomicCell;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI32, Ordering},
};
use uuid::Uuid;

use pumpkin_data::BlockState;
use pumpkin_data::entity::EntityType;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::zombie_nautilus_variant::ZombieNautilusVariant;
use pumpkin_data::{
    data_component_impl::EquipmentSlot,
    effect::StatusEffect,
    entity::EntityStatus,
    item_stack::ItemStack,
    particle::Particle,
    potion::Effect,
    sound::{Sound, SoundCategory},
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        brain::{Brain, BrainTick, memory::PackedMemories},
        control::{
            smooth_swimming_look_control::SmoothSwimmingLookControl,
            smooth_swimming_move_control::SmoothSwimmingMoveControl,
        },
        pathfinder::{Navigator, node::PathType},
    },
    custom_sound::CustomSound,
    mob::{
        Mob, MobEntity,
        equipment::equip_on_target,
        spawn::{EntitySpawnReason, SpawnGroupData},
    },
    passive::{animal::Animal, nautilus_ai},
    player::Player,
};
use crate::world::World;

const SMALL_RESTRICTION_RADIUS: i32 = 16;
const LARGE_RESTRICTION_RADIUS: i32 = 32;
const RESTRICTION_RADIUS_BUFFER: i32 = 8;
const NAUTILUS_WATER_RESISTANCE: f64 = 0.9;
const IN_WATER_SPEED_MODIFIER: f32 = 0.011;

pub struct NautilusEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: crate::entity::ageable::AgeableData,
    pub is_tame: AtomicBool,
    pub owner: AtomicCell<Option<Uuid>>,
    pub is_dashing: AtomicBool,
    pub dash_cooldown: AtomicI32,
    pub inventory: Mutex<Vec<ItemStack>>,
    /// Vanilla `ZombieNautilus` rather than `Nautilus`.
    zombie: bool,
    variant: AtomicI32,
}

/// Picks the water sound, or the on-land one.
const fn in_water(under_water: bool, water: Sound, land: Sound) -> Sound {
    if under_water { water } else { land }
}

impl NautilusEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let zombie = entity.entity_type == &EntityType::ZOMBIE_NAUTILUS;
        let mut mob_entity = MobEntity::new(entity);
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(
            SmoothSwimmingMoveControl::new(85, 10, IN_WATER_SPEED_MODIFIER, 0.0, true),
        );
        *mob_entity
            .look_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(SmoothSwimmingLookControl::new(10));
        {
            let navigator = mob_entity
                .navigator
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *navigator = Navigator::water_bound(false);
            navigator.set_pathfinding_malus(PathType::Water, 0.0);
        };
        let nautilus = Self {
            mob_entity,
            ageable_data: crate::entity::ageable::AgeableData::default(),
            is_tame: AtomicBool::new(false),
            owner: AtomicCell::new(None),
            is_dashing: AtomicBool::new(false),
            dash_cooldown: AtomicI32::new(0),
            inventory: Mutex::new(vec![ItemStack::new(0, &pumpkin_data::item::Item::AIR); 9]),
            zombie,
            variant: AtomicI32::new(i32::from(ZombieNautilusVariant::Temperate.id())),
        };

        Arc::new(nautilus)
    }

    pub fn is_dashing(&self) -> bool {
        self.is_dashing.load(Ordering::Relaxed)
    }

    pub fn set_dashing(&self, dashing: bool) {
        self.is_dashing.store(dashing, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::nautilus::DASH, dashing);
    }

    pub fn is_tame(&self) -> bool {
        self.is_tame.load(Ordering::Relaxed)
    }

    pub fn set_tame(&self, tame: bool, owner: Option<Uuid>) {
        self.is_tame.store(tame, Ordering::Relaxed);
        self.owner.store(owner);
    }

    #[must_use]
    pub const fn is_zombie(&self) -> bool {
        self.zombie
    }

    #[must_use]
    pub fn get_variant(&self) -> ZombieNautilusVariant {
        u8::try_from(self.variant.load(Ordering::Relaxed))
            .ok()
            .and_then(ZombieNautilusVariant::from_id)
            .unwrap_or_default()
    }

    pub fn set_variant_id(&self, raw_id: i32) {
        self.variant.store(raw_id, Ordering::Relaxed);
        if self.zombie {
            self.get_entity().set_synced_data(
                pumpkin_data::tracked_data::zombie_nautilus::DATA_VARIANT_ID,
                VarInt(raw_id),
            );
        }
    }

    fn under_water(&self) -> bool {
        self.get_entity().is_under_water()
    }

    fn is_baby_nautilus(&self) -> bool {
        self.mob_entity.living_entity.is_baby()
    }

    pub fn get_ambient_sound(&self) -> Sound {
        let water = self.under_water();
        if self.zombie {
            in_water(
                water,
                Sound::EntityZombieNautilusAmbient,
                Sound::EntityZombieNautilusAmbientLand,
            )
        } else if self.is_baby_nautilus() {
            in_water(
                water,
                Sound::EntityBabyNautilusAmbient,
                Sound::EntityBabyNautilusAmbientLand,
            )
        } else {
            in_water(
                water,
                Sound::EntityNautilusAmbient,
                Sound::EntityNautilusAmbientLand,
            )
        }
    }

    pub fn get_dash_sound(&self) -> Sound {
        let water = self.under_water();
        if self.zombie {
            in_water(
                water,
                Sound::EntityZombieNautilusDash,
                Sound::EntityZombieNautilusDashLand,
            )
        } else {
            in_water(
                water,
                Sound::EntityNautilusDash,
                Sound::EntityNautilusDashLand,
            )
        }
    }

    pub fn get_dash_ready_sound(&self) -> Sound {
        let water = self.under_water();
        if self.zombie {
            in_water(
                water,
                Sound::EntityZombieNautilusDashReady,
                Sound::EntityZombieNautilusDashReadyLand,
            )
        } else {
            in_water(
                water,
                Sound::EntityNautilusDashReady,
                Sound::EntityNautilusDashReadyLand,
            )
        }
    }

    /// Vanilla `isAggravated`: angry at someone or attacking.
    fn is_aggravated(&self) -> bool {
        self.mob_entity.with_brain(self, |brain| {
            brain.has_memory_value(crate::entity::ai::brain::memory::types::ANGRY_AT.id())
                || brain
                    .has_memory_value(crate::entity::ai::brain::memory::types::ATTACK_TARGET.id())
        })
    }

    /// Vanilla `isMobControlled`: the first passenger is a mob.
    fn is_mob_controlled(&self) -> bool {
        self.get_entity()
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first()
            .is_some_and(|passenger| passenger.get_mob().is_some())
    }

    /// Vanilla `AbstractNautilus.getNautilusRestrictionRadius`.
    fn restriction_radius(&self) -> i32 {
        let unsaddled = self
            .mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::SADDLE)
            .is_empty();
        if !self.mob_entity.living_entity.is_baby() && unsaddled {
            LARGE_RESTRICTION_RADIUS
        } else {
            SMALL_RESTRICTION_RADIUS
        }
    }

    /// Vanilla `AbstractNautilus.checkRestriction`: a tame nautilus roams around where it is.
    fn check_restriction(&self) {
        let entity = self.get_entity();
        if entity.is_leashed() || entity.has_passengers() || !self.is_tame() {
            return;
        }
        let radius = self.restriction_radius();
        let block_pos = entity.block_pos.load();
        let home_radius = self
            .mob_entity
            .position_target_range
            .load(Ordering::Relaxed);
        if home_radius >= 0 {
            let reach = f64::from(radius + RESTRICTION_RADIUS_BUFFER);
            let home = self.mob_entity.position_target.load();
            if f64::from(home.squared_distance(&block_pos)) < reach * reach && home_radius == radius
            {
                return;
            }
        }
        self.mob_entity.set_home_to(block_pos, radius);
    }

    pub fn get_eat_sound(&self) -> Sound {
        if self.zombie {
            Sound::EntityZombieNautilusEat
        } else if self.is_baby_nautilus() {
            Sound::EntityBabyNautilusEat
        } else {
            Sound::EntityNautilusEat
        }
    }
}

impl Animal for NautilusEntity {
    fn play_eating_sound(&self) {
        self.make_sound(self.get_eat_sound());
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_NAUTILUS_FOOD)
    }
}

impl CustomSound for NautilusEntity {
    fn hurt_sound(&self) -> Option<Sound> {
        let water = self.under_water();
        Some(if self.zombie {
            in_water(
                water,
                Sound::EntityZombieNautilusHurt,
                Sound::EntityZombieNautilusHurtLand,
            )
        } else if self.is_baby_nautilus() {
            in_water(
                water,
                Sound::EntityBabyNautilusHurt,
                Sound::EntityBabyNautilusHurtLand,
            )
        } else {
            in_water(
                water,
                Sound::EntityNautilusHurt,
                Sound::EntityNautilusHurtLand,
            )
        })
    }

    fn death_sound(&self) -> Option<Sound> {
        let water = self.under_water();
        Some(if self.zombie {
            in_water(
                water,
                Sound::EntityZombieNautilusDeath,
                Sound::EntityZombieNautilusDeathLand,
            )
        } else if self.is_baby_nautilus() {
            in_water(
                water,
                Sound::EntityBabyNautilusDeath,
                Sound::EntityBabyNautilusDeathLand,
            )
        } else {
            in_water(
                water,
                Sound::EntityNautilusDeath,
                Sound::EntityNautilusDeathLand,
            )
        })
    }
}

impl crate::entity::ageable::AgeableMob for NautilusEntity {
    fn get_ageable_data(&self) -> &crate::entity::ageable::AgeableData {
        &self.ageable_data
    }

    /// Vanilla `ZombieNautilus.canBeABaby`.
    fn can_be_a_baby(&self) -> bool {
        !self.zombie
    }
}

impl Mob for NautilusEntity {
    /// Vanilla `AbstractNautilus.removeWhenFarAway`: despawns like a monster, unlike animals.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        true
    }

    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(Self::get_ambient_sound(self))
    }

    fn as_ageable(&self) -> Option<&dyn crate::entity::ageable::AgeableMob> {
        Some(self)
    }

    fn can_dispenser_equip_into_slot(&self, slot: &EquipmentSlot) -> bool {
        *slot == EquipmentSlot::BODY
            || *slot == EquipmentSlot::SADDLE
            || self.mob_entity.can_pick_up_loot()
    }

    fn is_tamed(&self) -> bool {
        self.is_tame()
    }

    fn get_owner_uuid(&self) -> Option<Uuid> {
        self.owner.load()
    }

    fn mob_controlling_passenger(&self) -> Option<Arc<dyn EntityBase>> {
        self.saddled_controlling_passenger(None)
    }

    fn mob_get_swim_sound(&self) -> Option<Sound> {
        Some(if self.zombie {
            Sound::EntityZombieNautilusSwim
        } else if self.is_baby_nautilus() {
            Sound::EntityBabyNautilusSwim
        } else {
            Sound::EntityNautilusSwim
        })
    }

    /// Vanilla `AbstractNautilus` steps silently.
    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {}

    fn as_custom_sound(&self) -> Option<&dyn crate::entity::custom_sound::CustomSound> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn can_be_leashed(&self) -> bool {
        !(self.is_aggravated() || (self.zombie && self.is_mob_controlled()))
    }

    /// Vanilla `ZombieNautilus.sunProtectionSlot`: body armor shades it.
    fn sun_protection_slot(&self) -> EquipmentSlot {
        if self.zombie {
            EquipmentSlot::BODY
        } else {
            EquipmentSlot::HEAD
        }
    }

    fn mob_get_component(
        &self,
        component: pumpkin_data::data_component::DataComponent,
    ) -> Option<Box<dyn pumpkin_data::data_component_impl::DataComponentImpl>> {
        use pumpkin_data::data_component::DataComponent;
        use pumpkin_data::data_component_impl::ZombieNautilusVariantImpl;
        match component {
            DataComponent::ZombieNautilusVariant if self.zombie => {
                Some(Box::new(ZombieNautilusVariantImpl {
                    value: format!("minecraft:{}", self.get_variant().to_name()).into(),
                }))
            }
            _ => None,
        }
    }

    fn mob_set_variant_name(&self, name: &str) {
        if self.zombie
            && let Some(id) =
                self.resolve_dynamic_raw_id("minecraft:zombie_nautilus_variant", name, |n| {
                    ZombieNautilusVariant::from_name(n).map(|v| u32::from(v.id()))
                })
        {
            self.set_variant_id(id as i32);
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if self.zombie {
            let id = self.variant.load(Ordering::Relaxed) as u32;
            let name =
                self.resolve_dynamic_entry_id("minecraft:zombie_nautilus_variant", id, |_| {
                    format!("minecraft:{}", self.get_variant().to_name())
                });
            nbt.put_string("variant", name);
        }
        nbt.put_bool("IsTame", self.is_tame.load(Ordering::Relaxed));
        nbt.put_int("DashCooldown", self.dash_cooldown.load(Ordering::Relaxed));
        if let Some(owner) = self.owner.load() {
            nbt.put_uuid("Owner", owner);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if self.zombie
            && let Some(name) = nbt.get_string("variant")
        {
            self.mob_set_variant_name(name);
        }
        self.mob_entity
            .read_legacy_saddle(nbt.get_bool("Saddled") == Some(true));
        if let Some(is_tame) = nbt.get_bool("IsTame") {
            self.is_tame.store(is_tame, Ordering::Relaxed);
        }
        if let Some(dash) = nbt.get_int("DashCooldown") {
            self.dash_cooldown.store(dash, Ordering::Relaxed);
        }
        if let Some(owner) = nbt.get_uuid("Owner") {
            self.owner.store(Some(owner));
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        if self.zombie {
            nautilus_ai::make_zombie_brain(self, packed)
        } else {
            nautilus_ai::make_brain(self, packed)
        }
    }

    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        nautilus_ai::update_activity(tick.state);
    }

    /// Vanilla `Nautilus.customServerAiStep`, then `AbstractNautilus.customServerAiStep`.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        // Vanilla `getTarget` reads the brain's attack target.
        let target = self.mob_entity.with_brain(self, |brain| {
            brain
                .get(crate::entity::ai::brain::memory::types::ATTACK_TARGET)
                .cloned()
        });
        self.mob_entity.set_target(target);
        self.check_restriction();
    }

    /// Vanilla `AbstractNautilus.travelInWater`.
    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        entity.update_velocity_from_input(
            living.movement_input.load(),
            f64::from(living.get_speed()),
        );
        living.make_move(caller);
        entity
            .velocity
            .store(entity.velocity.load() * NAUTILUS_WATER_RESISTANCE);
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        _reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if self.zombie {
            let biome = world.get_biome(&self.get_entity().block_pos.load());
            self.finalize_spawn_in_biome(biome);
        }
        self.mob_entity
            .with_brain_tick(self, nautilus_ai::init_memories);
        self.finalize_spawn_super(group_data)
    }

    /// Vanilla `AbstractNautilus.hurtServer`: turns on a living attacker.
    fn on_damage_with_context(
        &self,
        _damage_type: pumpkin_data::damage::DamageType,
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
        if attacker.get_living_entity().is_none() {
            return true;
        }
        self.mob_entity.post_to_brain(Box::new(move |tick| {
            nautilus_ai::set_anger_target(tick, attacker.as_ref());
        }));
        true
    }

    /// Vanilla `ZombieNautilus.finalizeSpawn`: the variant follows the biome.
    fn finalize_spawn_in_biome(&self, biome: &'static pumpkin_data::chunk::Biome) {
        if self.zombie {
            self.set_variant_id(i32::from(
                ZombieNautilusVariant::select_for_biome(biome.registry_id).id(),
            ));
        }
    }

    fn mob_init_data_tracker(&self) {
        if self.zombie {
            self.set_variant_id(self.variant.load(Ordering::Relaxed));
        }
        self.mob_entity.living_entity.entity.set_synced_data(
            pumpkin_data::tracked_data::nautilus::DASH,
            self.is_dashing(),
        );
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        tick_abstract_nautilus(
            &self.mob_entity,
            &self.dash_cooldown,
            self.is_dashing(),
            |dashing| self.set_dashing(dashing),
            || self.get_dash_ready_sound(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        interact_abstract_nautilus(
            self,
            player,
            item_stack,
            self.is_tame(),
            |owner| self.set_tame(true, Some(owner)),
            || self.get_eat_sound(),
        )
    }

    fn can_use_slot(&self, slot: &EquipmentSlot) -> bool {
        nautilus_can_use_slot(self, slot)
    }

    fn get_equip_sound(&self, slot: &EquipmentSlot) -> Option<Sound> {
        nautilus_equip_sound(self.get_entity(), slot)
    }
}

pub(super) fn tick_abstract_nautilus(
    mob_entity: &MobEntity,
    dash_cooldown: &AtomicI32,
    is_dashing: bool,
    set_dashing: impl FnOnce(bool),
    dash_ready_sound: impl FnOnce() -> Sound,
) {
    let entity = &mob_entity.living_entity.entity;

    if let Ok(passengers) = entity.passengers.try_lock()
        && let Some(passenger) = passengers.first()
        && let Some(player) = passenger.cast_any().downcast_ref::<Player>()
    {
        let world = entity.world.load();
        let has_effect = player
            .living_entity
            .has_effect(&StatusEffect::BREATH_OF_THE_NAUTILUS);
        if (!has_effect || world.get_world_age() % 40 == 0)
            && let Some(p) = world.get_player_by_uuid(player.gameprofile.id)
        {
            p.add_effect(Effect {
                effect_type: &StatusEffect::BREATH_OF_THE_NAUTILUS,
                duration: 60,
                amplifier: 0,
                ambient: true,
                show_particles: true,
                show_icon: true,
                blend: true,
            });
        }
    }

    if is_dashing && dash_cooldown.load(Ordering::Relaxed) < 35 {
        set_dashing(false);
    }

    let cooldown = dash_cooldown.load(Ordering::Relaxed);
    if cooldown > 0 {
        let next = cooldown - 1;
        dash_cooldown.store(next, Ordering::Relaxed);
        if next == 0 {
            entity.world.load().play_sound(
                dash_ready_sound(),
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
        }
    }

    if entity.touching_water.load(Ordering::Relaxed) {
        let velo = entity.velocity.load();
        let speed = velo.length();
        let prob = (speed * 2.0).clamp(0.15, 1.0);
        if rand::random::<f64>() < prob {
            let world = entity.world.load();
            let pos = entity.pos.load();
            world.spawn_particle(
                pos + Vector3::new(0.0, 0.25, 0.0),
                Vector3::new(0.4, 0.4, 0.4),
                0.5,
                2,
                Particle::Bubble,
            );
        }
    }
}

pub(super) fn interact_abstract_nautilus<M: Animal>(
    mob: &M,
    player: &Arc<Player>,
    item_stack: &mut ItemStack,
    is_tame: bool,
    on_tamed: impl FnOnce(Uuid),
    eat_sound: impl FnOnce() -> Sound,
) -> bool {
    use pumpkin_data::tag::{self, Taggable};

    let mob_entity = mob.get_mob_entity();
    let entity = &mob_entity.living_entity.entity;

    let consume_nautilus_food = |item_stack: &mut ItemStack| {
        if item_stack
            .item
            .has_tag(&tag::Item::MINECRAFT_NAUTILUS_BUCKET_FOOD)
        {
            crate::item::items::bucket::fill_bucket_stack(
                item_stack,
                player,
                &pumpkin_data::item::Item::WATER_BUCKET,
            );
        } else {
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
        }
    };

    if !is_tame && mob.is_food(item_stack) {
        consume_nautilus_food(item_stack);
        let world = entity.world.load();
        if rand::random::<u32>().is_multiple_of(3) && mob.approve_taming(player) {
            on_tamed(player.gameprofile.id);
            mob_entity
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .stop();
            world.send_entity_status(
                entity,
                EntityStatus::TamingSucceeded,
                ActorEventID::TamingSucceeded,
            );
        } else {
            world.send_entity_status(
                entity,
                EntityStatus::TamingFailed,
                ActorEventID::TamingFailed,
            );
        }
        world.play_sound(eat_sound(), SoundCategory::Neutral, &entity.pos.load());
        return true;
    }

    if !item_stack.is_empty()
        && mob.is_food(item_stack)
        && mob_entity.living_entity.health.load() < mob_entity.living_entity.get_max_health()
    {
        let heal = item_stack
            .get_data_component::<pumpkin_data::data_component_impl::FoodImpl>()
            .map_or(1.0, |food| 2.0 * food.nutrition as f32);
        consume_nautilus_food(item_stack);
        mob_entity.living_entity.heal(heal);
        entity
            .world
            .load()
            .play_sound(eat_sound(), SoundCategory::Neutral, &entity.pos.load());
        return true;
    }

    if is_tame && !player.get_entity().is_sneaking() {
        if equip_on_target(mob, player, item_stack) {
            return true;
        }

        if !mob.is_food(item_stack) {
            let world = player.world();
            if let Some(vehicle) = world.get_entity_by_id(entity.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                entity.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
                return true;
            }
        }
    }

    mob.animal_interact(player, item_stack)
}

pub(super) fn nautilus_can_use_slot(mob: &dyn Mob, slot: &EquipmentSlot) -> bool {
    (*slot != EquipmentSlot::SADDLE && *slot != EquipmentSlot::BODY)
        || (mob.get_entity().is_alive()
            && mob.as_ageable().is_none_or(|a| !a.is_baby())
            && mob.is_tamed())
}

pub(super) fn nautilus_equip_sound(entity: &Entity, slot: &EquipmentSlot) -> Option<Sound> {
    if *slot != EquipmentSlot::SADDLE {
        return None;
    }
    Some(if entity.is_under_water() {
        Sound::ItemNautilusSaddleUnderwaterEquip
    } else {
        Sound::ItemNautilusSaddleEquip
    })
}
