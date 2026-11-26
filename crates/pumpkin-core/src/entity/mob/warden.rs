use std::sync::{Arc, Mutex, atomic::Ordering};

use pumpkin_data::BlockState;
use pumpkin_data::{
    damage::DamageType,
    effect::StatusEffect,
    entity::{EntityPose, EntityStatus, EntityType},
    game_event::GameEvent,
    potion::Effect,
    sound::Sound,
    tag::{self, Taggable},
    tracked_data,
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::{GameMode, math::vector3::Vector3};

use super::warden_ai;
use super::warden_anger::{AngerLevel, AngerManagement};
use crate::entity::{
    Entity, EntityBase,
    ai::brain::{Brain, BrainState, memory::PackedMemories, memory::types},
    ai::pathfinder::{Navigator, node::PathType},
    mob::{
        Mob, MobEntity,
        spawn::{EntitySpawnReason, SpawnGroupData},
    },
    player::Player,
    vibration::{self, VibrationData, VibrationListener, VibrationUser},
};
use crate::world::{
    World,
    game_event::{GameEventContext, GameEventListener},
};

const DARKNESS_DISPLAY_LIMIT: i32 = 200;
const DARKNESS_DURATION: i32 = 260;

pub struct WardenEntity {
    pub mob_entity: MobEntity,
    anger_management: Mutex<AngerManagement>,
    vibration: Mutex<VibrationData>,
}

impl WardenEntity {
    pub const XP_REWARD: u32 = 5;
    /// Vanilla `Warden.FOLLOW_RANGE`.
    pub const FOLLOW_RANGE: f32 = 24.0;
    /// Vanilla `EntityAttachment.WARDEN_CHEST` of the warden type.
    pub const CHEST_ATTACHMENT: Vector3<f64> = Vector3::new(0.0, 1.6, 0.0);
    const VIBRATION_COOLDOWN_TICKS: i64 = 40;
    const TIME_TO_USE_MELEE_UNTIL_SONIC_BOOM: i64 = 200;
    const DARKNESS_RADIUS: f64 = 20.0;
    const DARKNESS_INTERVAL: i32 = 120;
    const ANGERMANAGEMENT_TICK_DELAY: i32 = 20;
    const DEFAULT_ANGER: i32 = 35;
    const PROJECTILE_ANGER: i32 = 10;
    const ON_HURT_ANGER_BOOST: i32 = 20;
    const RECENT_PROJECTILE_TICK_THRESHOLD: i64 = 100;
    const TOUCH_COOLDOWN_TICKS: i64 = 20;
    const PROJECTILE_ANGER_DISTANCE: f64 = 30.0;
    const DIRECT_ATTACK_RANGE: f64 = 5.0;

    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        {
            let navigator = mob_entity
                .navigator
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *navigator = Navigator::warden();
            navigator.set_can_float(true);
            navigator.set_pathfinding_malus(PathType::UnpassableRail, 0.0);
            navigator.set_pathfinding_malus(PathType::DamageOther, 8.0);
            navigator.set_pathfinding_malus(PathType::PowderSnow, 8.0);
            navigator.set_pathfinding_malus(PathType::Lava, 8.0);
            navigator.set_pathfinding_malus(PathType::DamageFire, 0.0);
            navigator.set_pathfinding_malus(PathType::DangerFire, 0.0);
        };
        Arc::new(Self {
            mob_entity,
            anger_management: Mutex::new(AngerManagement::default()),
            vibration: Mutex::new(VibrationData::default()),
        })
    }

    /// The sculk shrieker spawn: vanilla `finalizeSpawn` with `EntitySpawnReason.TRIGGERED`.
    pub fn emerge(&self) {
        let world = self.get_entity().world.load_full();
        self.finalize_spawn(&world, EntitySpawnReason::Triggered, None);
    }

    fn anger(&self) -> std::sync::MutexGuard<'_, AngerManagement> {
        self.anger_management
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn is_digging_or_emerging(&self) -> bool {
        matches!(
            self.get_entity().pose.load(),
            EntityPose::Digging | EntityPose::Emerging
        )
    }

    /// Vanilla `Warden.canTargetEntity`.
    pub fn can_target_entity(&self, entity: &dyn EntityBase) -> bool {
        let Some(living) = entity.get_living_entity() else {
            return false;
        };
        let target = entity.get_entity();
        let world = self.get_entity().world.load();
        let this: &dyn EntityBase = self;
        Arc::ptr_eq(&world, &target.world.load())
            && !entity.is_spectator()
            && !entity.get_player().is_some_and(Player::is_creative)
            && !this.is_allied_to(entity)
            && target.entity_type != &EntityType::ARMOR_STAND
            && target.entity_type != &EntityType::WARDEN
            && !target.invulnerable.load(Ordering::Relaxed)
            && !living.is_dead_or_dying()
            && world
                .worldborder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_within_bounds_box(&target.bounding_box.load())
    }

    /// Vanilla `Warden.getActiveAnger`, with `getTarget` read from the brain.
    fn active_anger(&self, state: &BrainState) -> i32 {
        let target = state.get(types::ATTACK_TARGET).cloned();
        self.anger().get_active_anger(target.as_deref())
    }

    /// Vanilla `Warden.getAngerLevel`.
    pub fn anger_level_in(&self, state: &BrainState) -> AngerLevel {
        AngerLevel::by_anger(self.active_anger(state))
    }

    /// [`Self::anger_level_in`] from outside a brain tick.
    pub fn anger_level(&self) -> AngerLevel {
        self.mob_entity
            .with_brain(self, |brain| self.anger_level_in(brain))
    }

    /// Vanilla `Warden.getEntityAngryAt`.
    pub fn get_entity_angry_at_in(&self, state: &BrainState) -> Option<Arc<dyn EntityBase>> {
        if !self.anger_level_in(state).is_angry() {
            return None;
        }
        self.anger()
            .get_active_entity(|entity| self.can_target_entity(entity))
    }

    pub fn clear_anger(&self, entity: &dyn EntityBase) {
        self.anger().clear_anger(entity);
    }

    /// Vanilla `Warden.increaseAngerAt(entity)`.
    pub fn increase_anger_at_default(&self, state: &mut BrainState, entity: &dyn EntityBase) {
        self.increase_anger_at(state, entity, Self::DEFAULT_ANGER, true);
    }

    /// Vanilla `Warden.increaseAngerAt(entity, amount, playListeningSound)`.
    pub fn increase_anger_at(
        &self,
        state: &mut BrainState,
        entity: &dyn EntityBase,
        amount: i32,
        play_listening_sound: bool,
    ) {
        if self.mob_entity.is_no_ai() || !self.can_target_entity(entity) {
            return;
        }
        warden_ai::set_dig_cooldown(state);
        let was_not_targeting_player = state
            .get(types::ATTACK_TARGET)
            .is_none_or(|target| target.get_player().is_none());
        let Some(entity) = self
            .get_entity()
            .world
            .load()
            .get_entity_by_id(entity.get_entity().entity_id)
        else {
            return;
        };
        // Bukkit `WardenAngerChangeEvent`: plugins may cancel or set the new anger.
        let old_anger = self.anger().anger_of(entity.as_ref());
        let mut event =
            crate::plugin::api::events::entity::warden_anger_change::WardenAngerChangeEvent {
                entity_id: self.get_entity().entity_id,
                target_id: entity.get_entity().entity_id,
                old_anger,
                new_anger: (old_anger + amount).min(super::warden_anger::MAX_ANGER),
                cancelled: false,
            };
        if self.get_entity().world.load().fire_plugin_event(&mut event) {
            return;
        }
        let amount = event.new_anger.clamp(0, super::warden_anger::MAX_ANGER) - old_anger;
        let anger = self.anger().increase_anger(&entity, amount);
        if entity.get_player().is_some()
            && was_not_targeting_player
            && AngerLevel::by_anger(anger).is_angry()
        {
            state.erase(types::ATTACK_TARGET.id());
        }
        if play_listening_sound {
            self.play_listening_sound(state);
        }
    }

    /// Vanilla `Warden.playListeningSound`.
    fn play_listening_sound(&self, state: &BrainState) {
        if self.get_entity().pose.load() != EntityPose::Roaring {
            let pitch = self.mob_entity.living_entity.get_pitch();
            self.play_entity_sound(self.anger_level_in(state).listening_sound(), 10.0, pitch);
        }
    }

    /// Vanilla `Warden.setAttackTarget`.
    pub fn set_attack_target(state: &mut BrainState, target: Arc<dyn EntityBase>) {
        state.erase(types::ROAR_TARGET.id());
        state.set(types::ATTACK_TARGET, target);
        state.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
        warden_ai::set_sonic_boom_cooldown(state, Self::TIME_TO_USE_MELEE_UNTIL_SONIC_BOOM);
    }

    /// Vanilla `Warden.syncClientAngerLevel`.
    fn sync_client_anger_level(&self, state: &BrainState) {
        self.get_entity().set_synced_data(
            tracked_data::warden::CLIENT_ANGER_LEVEL,
            self.active_anger(state),
        );
    }

    pub fn apply_darkness_around(world: &World, position: Vector3<f64>, radius: f64) {
        for player in world.get_nearby_players(position, radius) {
            if !matches!(
                player.gamemode.load(),
                GameMode::Survival | GameMode::Adventure
            ) {
                continue;
            }
            let current = player.living_entity.get_effect(&StatusEffect::DARKNESS);
            if current.is_some_and(|effect| {
                effect.duration < 0 || effect.duration >= DARKNESS_DISPLAY_LIMIT
            }) {
                continue;
            }
            let darkness = Effect {
                effect_type: &StatusEffect::DARKNESS,
                duration: DARKNESS_DURATION,
                amplifier: 0,
                ambient: false,
                show_particles: false,
                show_icon: false,
                blend: true,
            };
            player.send_effect(&darkness);
            player.living_entity.add_effect(darkness);
        }
    }
}

/// Vanilla `Warden.VibrationUser`.
impl VibrationUser for WardenEntity {
    fn vibration_data(&self) -> &Mutex<VibrationData> {
        &self.vibration
    }

    fn vibration_world(&self) -> Option<std::sync::Arc<crate::world::World>> {
        Some(self.get_entity().world.load_full())
    }

    fn vibration_position(&self) -> pumpkin_util::math::vector3::Vector3<f64> {
        self.get_eye_pos()
    }

    fn vibration_destination(&self) -> crate::entity::vibration::VibrationDestination {
        let entity = self.get_entity();
        #[expect(clippy::cast_possible_truncation, reason = "the codec sends a float")]
        crate::entity::vibration::VibrationDestination::Entity {
            id: entity.entity_id,
            eye_offset: entity.get_eye_height() as f32,
        }
    }

    fn listenable_events(&self) -> &'static tag::Tag {
        &tag::GameEvent::MINECRAFT_WARDEN_CAN_LISTEN
    }

    fn can_trigger_avoid_vibration(&self) -> bool {
        true
    }

    fn can_receive_vibration(
        &self,
        world: &World,
        pos: BlockPos,
        _event: GameEvent,
        context: &GameEventContext<'_>,
    ) -> bool {
        if self.mob_entity.is_no_ai()
            || self.mob_entity.living_entity.is_dead_or_dying()
            || self.mob_entity.with_brain(self, |brain| {
                brain.has_memory_value(types::VIBRATION_COOLDOWN.id())
            })
            || self.is_digging_or_emerging()
            || !world
                .worldborder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_block(pos.0.x, pos.0.z)
        {
            return false;
        }
        context.source_entity.is_none_or(|source| {
            source.get_living_entity().is_none() || self.can_target_entity(source)
        })
    }

    fn on_receive_vibration(
        &self,
        world: &std::sync::Arc<World>,
        pos: BlockPos,
        _event: GameEvent,
        source: Option<Arc<dyn EntityBase>>,
        projectile_owner: Option<Arc<dyn EntityBase>>,
        _distance: f32,
    ) {
        if self.mob_entity.living_entity.is_dead_or_dying() {
            return;
        }
        let entity = self.get_entity();
        self.mob_entity.with_brain(self, |brain| {
            brain.set_with_expiry(
                types::VIBRATION_COOLDOWN,
                (),
                Self::VIBRATION_COOLDOWN_TICKS,
            );
            world.send_entity_status(entity, EntityStatus::TendrilsShiver, ActorEventID::None);
            let pitch = self.mob_entity.living_entity.get_pitch();
            self.play_entity_sound(Sound::EntityWardenTendrilClicks, 5.0, pitch);
            let mut disturbance = pos;
            if let Some(owner) = &projectile_owner {
                let owner_pos = owner.get_entity().pos.load();
                if entity.pos.load().squared_distance_to_vec(&owner_pos)
                    < Self::PROJECTILE_ANGER_DISTANCE * Self::PROJECTILE_ANGER_DISTANCE
                {
                    if brain.has_memory_value(types::RECENT_PROJECTILE.id()) {
                        if self.can_target_entity(owner.as_ref()) {
                            disturbance = owner.get_entity().block_pos.load();
                        }
                        self.increase_anger_at_default(brain, owner.as_ref());
                    } else {
                        self.increase_anger_at(brain, owner.as_ref(), Self::PROJECTILE_ANGER, true);
                    }
                }
                brain.set_with_expiry(
                    types::RECENT_PROJECTILE,
                    (),
                    Self::RECENT_PROJECTILE_TICK_THRESHOLD,
                );
            } else if let Some(source) = &source {
                self.increase_anger_at_default(brain, source.as_ref());
            }
            if !self.anger_level_in(brain).is_angry() {
                let active = self
                    .anger()
                    .get_active_entity(|entity| self.can_target_entity(entity));
                let source_is_active = |active: &Arc<dyn EntityBase>| {
                    source.as_ref().is_some_and(|source| {
                        active.get_entity().entity_id == source.get_entity().entity_id
                    })
                };
                if projectile_owner.is_some() || active.as_ref().is_none_or(source_is_active) {
                    warden_ai::set_disturbance_location(self, brain, disturbance);
                }
            }
        });
    }
}

impl Mob for WardenEntity {
    /// Vanilla `Warden.removeWhenFarAway`: never despawns for distance.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        false
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        warden_ai::make_brain(self, packed)
    }

    /// Vanilla `Warden.customServerAiStep`.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        let entity = self.get_entity();
        let tick_count = entity.tick_count.load(Ordering::Relaxed);
        let world = entity.world.load_full();
        if (tick_count + entity.entity_id) % Self::DARKNESS_INTERVAL == 0 {
            Self::apply_darkness_around(&world, entity.pos.load(), Self::DARKNESS_RADIUS);
        }
        let target = self.mob_entity.with_brain(self, |brain| {
            if tick_count % Self::ANGERMANAGEMENT_TICK_DELAY == 0 {
                self.anger()
                    .tick(&world, |entity| self.can_target_entity(entity));
                self.sync_client_anger_level(brain);
            }
            warden_ai::update_activity(brain);
            // Vanilla `getTarget` reads the brain's attack target.
            brain.get(types::ATTACK_TARGET).cloned()
        });
        self.mob_entity.set_target(target);
    }

    /// Vanilla `Warden.tick`: the vibration listener, and persistent wardens never dig away.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        vibration::tick(self);
        if self.mob_entity.persistence_required.load(Ordering::Relaxed)
            || self.requires_custom_persistence()
        {
            self.mob_entity
                .with_brain(self, |brain| warden_ai::set_dig_cooldown(brain));
        }
    }

    fn for_each_game_event_listener(&self, visit: &mut dyn FnMut(&dyn GameEventListener)) {
        visit(&VibrationListener(self));
    }

    /// Vanilla `Warden.finalizeSpawn`.
    fn finalize_spawn(
        &self,
        _world: &Arc<World>,
        reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.mob_entity.with_brain(self, |brain| {
            brain.set_with_expiry(types::DIG_COOLDOWN, (), warden_ai::DIGGING_COOLDOWN);
            if reason == EntitySpawnReason::Triggered {
                self.get_entity().set_pose(EntityPose::Emerging);
                brain.set_with_expiry(
                    types::IS_EMERGING,
                    (),
                    i64::from(warden_ai::EMERGE_DURATION),
                );
                self.play_entity_sound(Sound::EntityWardenAgitated, 5.0, 1.0);
            }
        });
        self.finalize_spawn_super(group_data)
    }

    /// Vanilla `Warden.isInvulnerableTo` while digging or emerging.
    fn mob_hurt_server(
        &self,
        _amount: f32,
        damage_type: DamageType,
        _position: Option<Vector3<f64>>,
        _source: Option<&dyn EntityBase>,
        _cause: Option<&dyn EntityBase>,
    ) -> Option<bool> {
        (self.is_digging_or_emerging()
            && !damage_type.has_tag(&tag::DamageType::MINECRAFT_BYPASSES_INVULNERABILITY))
        .then_some(false)
    }

    /// Vanilla `Warden.hurtServer`, after the damage landed.
    fn on_damage_with_context(
        &self,
        _damage_type: DamageType,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        if self.mob_entity.is_no_ai() || self.is_digging_or_emerging() {
            return true;
        }
        let world = self.get_entity().world.load();
        let Some(attacker) =
            cause.and_then(|cause| world.get_entity_by_id(cause.get_entity().entity_id))
        else {
            return true;
        };
        let direct = source
            .is_some_and(|source| source.get_entity().entity_id == attacker.get_entity().entity_id);
        let close = self
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&attacker.get_entity().pos.load())
            < Self::DIRECT_ATTACK_RANGE * Self::DIRECT_ATTACK_RANGE;
        // Queued: the warden can be hurt during its own brain tick, by thorns.
        self.mob_entity.post_to_brain(Box::new(move |tick| {
            let Some(warden) = tick.mob.cast_any().downcast_ref::<Self>() else {
                return;
            };
            warden.increase_anger_at(
                tick.state,
                attacker.as_ref(),
                AngerLevel::Angry.minimum_anger() + Self::ON_HURT_ANGER_BOOST,
                false,
            );
            if !tick.state.has_memory_value(types::ATTACK_TARGET.id())
                && attacker.get_living_entity().is_some()
                && (direct || close)
            {
                Self::set_attack_target(tick.state, attacker);
            }
        }));
        true
    }

    /// Vanilla `Warden.doHurtTarget`.
    fn do_hurt_target(&self, target: &dyn EntityBase) -> bool {
        let entity = self.get_entity();
        entity.world.load().send_entity_status(
            entity,
            EntityStatus::StartAttacking,
            ActorEventID::StartAttacking,
        );
        let pitch = self.mob_entity.living_entity.get_pitch();
        self.play_entity_sound(Sound::EntityWardenAttackImpact, 10.0, pitch);
        // Called from the brain's melee attack, so the cooldown goes through the inbox.
        self.mob_entity.post_to_brain(Box::new(|tick| {
            warden_ai::set_sonic_boom_cooldown(tick.state, warden_ai::SONIC_BOOM_COOLDOWN);
        }));
        self.get_mob_entity().try_attack(entity, target)
    }

    /// Vanilla `Warden.doPush`.
    fn do_push(&self, entity: &Arc<dyn EntityBase>) {
        if self.mob_entity.is_no_ai() {
            return;
        }
        self.mob_entity.with_brain(self, |brain| {
            if brain.has_memory_value(types::TOUCH_COOLDOWN.id()) {
                return;
            }
            brain.set_with_expiry(types::TOUCH_COOLDOWN, (), Self::TOUCH_COOLDOWN_TICKS);
            self.increase_anger_at_default(brain, entity.as_ref());
            warden_ai::set_disturbance_location(self, brain, entity.get_entity().block_pos.load());
        });
    }

    fn mob_is_pushable(&self) -> bool {
        !self.is_digging_or_emerging() && self.mob_entity.living_entity.is_pushable()
    }

    /// Vanilla `Warden.getAmbientSound`.
    fn get_ambient_sound(&self) -> Option<Sound> {
        if self.get_entity().pose.load() == EntityPose::Roaring || self.is_digging_or_emerging() {
            return None;
        }
        Some(self.anger_level().ambient_sound())
    }

    fn mob_dampens_vibrations(&self) -> bool {
        true
    }

    fn xp_reward(&self) -> u32 {
        Self::XP_REWARD
    }

    fn mob_init_data_tracker(&self) {
        self.mob_entity
            .with_brain(self, |brain| self.sync_client_anger_level(brain));
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_compound("anger", self.anger().to_nbt());
        nbt.put_compound(
            "listener",
            self.vibration
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .write_nbt(),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(anger) = nbt.get_compound("anger") {
            *self.anger() = AngerManagement::from_nbt(anger);
        }
        if let Some(listener) = nbt.get_compound("listener") {
            *self
                .vibration
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                VibrationData::read_nbt(listener);
        }
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntityWardenStep, 10.0, 1.0);
    }

    fn check_spawn_obstruction(&self, world: &World) -> bool {
        let entity = &self.mob_entity.living_entity.entity;
        let bounding_box = entity.bounding_box.load();
        !world.contains_any_liquid(bounding_box)
            && world.get_entities_at_box(&bounding_box).is_empty()
            && world.is_space_empty(bounding_box)
    }
}
