use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::BlockState;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityStatus;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::java::client::play::CEntityStatus;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::block::entities::creaking_heart::CreakingHeartBlockEntity;
use crate::entity::{
    Entity, EntityBase,
    ai::brain::{
        Brain, BrainProvider, BrainTick,
        activity::ActivityData,
        behavior::{
            Behavior, DoNothing, LookAtTargetSink, MoveToTargetSink, Swim, Timed, melee_attack,
            random_stroll, run_one, set_entity_look_target_sometimes,
            set_walk_target_from_attack_target_if_target_out_of_reach,
            set_walk_target_from_look_target, start_attacking, stop_attacking_if_target_invalid,
        },
        memory::{MemoryModuleId, MemoryStatus, PackedMemories, types},
        sensing::SensorType,
    },
    mob::{Mob, MobEntity},
    player::Player,
};

pub const ATTACK_ANIMATION_DURATION: i32 = 15;
pub const MAX_HEALTH: f32 = 1.0;
pub const ATTACK_DAMAGE: f32 = 3.0;
pub const FOLLOW_RANGE: f32 = 32.0;
pub const ACTIVATION_RANGE_SQ: f64 = 144.0;
pub const ATTACK_INTERVAL: i32 = 40;
pub const MOVEMENT_SPEED_WHEN_FIGHTING: f32 = 0.4;
pub const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.3;
pub const CREAKING_ORANGE: i32 = 16545810;
pub const CREAKING_GRAY: i32 = 6250335;
pub const INVULNERABILITY_ANIMATION_DURATION: i32 = 8;
pub const TWITCH_DEATH_DURATION: i32 = 45;
pub const MAX_PLAYER_STUCK_COUNTER: i32 = 4;

pub struct CreakingEntity {
    pub mob_entity: MobEntity,
    can_move: AtomicBool,
    is_active: AtomicBool,
    is_tearing_down: AtomicBool,
    home_pos: AtomicCell<Option<BlockPos>>,
    attack_animation_remaining_ticks: AtomicI32,
    invulnerability_animation_remaining_ticks: AtomicI32,
    eyes_glowing: AtomicBool,
    next_flicker_time: AtomicI32,
    player_stuck_counter: AtomicI32,
    death_time: AtomicI32,
}

impl CreakingEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let creaking = Self {
            mob_entity,
            can_move: AtomicBool::new(true),
            is_active: AtomicBool::new(false),
            is_tearing_down: AtomicBool::new(false),
            home_pos: AtomicCell::new(None),
            attack_animation_remaining_ticks: AtomicI32::new(0),
            invulnerability_animation_remaining_ticks: AtomicI32::new(0),
            eyes_glowing: AtomicBool::new(false),
            next_flicker_time: AtomicI32::new(0),
            player_stuck_counter: AtomicI32::new(0),
            death_time: AtomicI32::new(0),
        };

        // Initialize attributes
        {
            let mut attributes = creaking
                .mob_entity
                .living_entity
                .attributes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if let Some(health) = attributes.get_mut(&Attributes::MAX_HEALTH.id) {
                health.base_value = f64::from(MAX_HEALTH);
                health.dirty.store(true, Ordering::Relaxed);
            }
            if let Some(speed) = attributes.get_mut(&Attributes::MOVEMENT_SPEED.id) {
                speed.base_value = f64::from(MOVEMENT_SPEED_WHEN_FIGHTING);
                speed.dirty.store(true, Ordering::Relaxed);
            }
            if let Some(damage) = attributes.get_mut(&Attributes::ATTACK_DAMAGE.id) {
                damage.base_value = f64::from(ATTACK_DAMAGE);
                damage.dirty.store(true, Ordering::Relaxed);
            }
            if let Some(follow) = attributes.get_mut(&Attributes::FOLLOW_RANGE.id) {
                follow.base_value = f64::from(FOLLOW_RANGE);
                follow.dirty.store(true, Ordering::Relaxed);
            }
            if let Some(step) = attributes.get_mut(&Attributes::STEP_HEIGHT.id) {
                step.base_value = 1.0625;
                step.dirty.store(true, Ordering::Relaxed);
            }
        }
        creaking.mob_entity.living_entity.health.store(MAX_HEALTH);

        Arc::new(creaking)
    }

    pub fn set_transient(&self, pos: BlockPos) {
        self.set_home_pos(Some(pos));
    }

    pub fn is_heart_bound(&self) -> bool {
        self.get_home_pos().is_some()
    }

    pub fn can_move(&self) -> bool {
        self.can_move.load(Ordering::Relaxed)
    }

    pub fn set_can_move(&self, can_move: bool) {
        self.can_move.store(can_move, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::creaking::CAN_MOVE, can_move);
    }

    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Relaxed)
    }

    pub fn set_is_active(&self, active: bool) {
        self.is_active.store(active, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::creaking::IS_ACTIVE, active);
    }

    pub fn is_tearing_down(&self) -> bool {
        self.is_tearing_down.load(Ordering::Relaxed)
    }

    pub fn set_tearing_down(&self) {
        self.is_tearing_down.store(true, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::creaking::IS_TEARING_DOWN, true);
    }

    pub fn get_home_pos(&self) -> Option<BlockPos> {
        self.home_pos.load()
    }

    pub fn set_home_pos(&self, pos: Option<BlockPos>) {
        self.home_pos.store(pos);
        if let Some(p) = pos {
            self.mob_entity.position_target.store(p);
            self.mob_entity
                .position_target_range
                .store(32, Ordering::Relaxed);
        } else {
            self.mob_entity
                .position_target_range
                .store(-1, Ordering::Relaxed);
        }
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::creaking::HOME_POS, pos);
    }

    pub fn stop_in_place(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        entity.velocity.store(Vector3::default());
        self.mob_entity.steered().navigator().stop();
    }

    /// Vanilla `activate`: the player becomes the brain's attack target.
    pub fn activate(&self, player: &Arc<Player>) {
        let target: Arc<dyn EntityBase> = player.clone();
        self.mob_entity.post_to_brain(Box::new(move |tick| {
            tick.state.set(types::ATTACK_TARGET, target);
        }));
        self.play_sound(Sound::EntityCreakingActivate);
        self.set_is_active(true);
    }

    pub fn deactivate(&self) {
        self.mob_entity.post_to_brain(Box::new(|tick| {
            tick.state.erase(types::ATTACK_TARGET.id());
        }));
        self.play_sound(Sound::EntityCreakingDeactivate);
        self.set_is_active(false);
    }

    pub fn has_glowing_eyes(&self) -> bool {
        self.eyes_glowing.load(Ordering::Relaxed)
    }

    pub fn check_eye_blink(&self) {
        let death_time = self.death_time.load(Ordering::Relaxed);
        let next_flicker = self.next_flicker_time.load(Ordering::Relaxed);
        if death_time > next_flicker {
            let glowing = self.eyes_glowing.load(Ordering::Relaxed);
            let min_ticks = if glowing { 2 } else { (death_time / 4).max(1) };
            let max_ticks = if glowing { 8 } else { (death_time / 2).max(2) };
            let added_ticks =
                (rand::random::<u32>() as i32 % (max_ticks - min_ticks + 1)).abs() + min_ticks;
            self.next_flicker_time
                .store(death_time + added_ticks, Ordering::Relaxed);
            self.eyes_glowing.store(!glowing, Ordering::Relaxed);
        }
    }

    pub fn player_is_stuck_in_you(&self, players: &[Arc<Player>]) -> bool {
        let own_box = self.mob_entity.living_entity.entity.bounding_box.load();
        for player in players {
            let p = player.get_entity().get_eye_pos();
            if p.x >= own_box.min.x
                && p.x <= own_box.max.x
                && p.y >= own_box.min.y
                && p.y <= own_box.max.y
                && p.z >= own_box.min.z
                && p.z <= own_box.max.z
            {
                let count = self.player_stuck_counter.fetch_add(1, Ordering::Relaxed) + 1;
                return count > MAX_PLAYER_STUCK_COUNTER;
            }
        }
        self.player_stuck_counter.store(0, Ordering::Relaxed);
        false
    }

    pub fn is_looking_at_me(&self, player: &Player) -> bool {
        let creaking_entity = &self.mob_entity.living_entity.entity;
        let creaking_pos = creaking_entity.pos.load();
        let eye_y = creaking_entity.get_eye_y();
        let feet_y = creaking_pos.y + 0.5;
        let mid_y = f64::midpoint(eye_y, creaking_pos.y);

        let player_entity = player.get_entity();
        let player_eye_pos = player_entity.get_eye_pos();

        let pitch = player_entity.pitch.load().to_radians();
        let yaw = -player_entity.yaw.load().to_radians();

        let cos_pitch = pitch.cos();
        let look_dir = Vector3::new(
            (yaw.sin() * cos_pitch) as f64,
            (-pitch.sin()) as f64,
            (yaw.cos() * cos_pitch) as f64,
        );

        let targets_y = [eye_y, feet_y, mid_y];
        for target_y in targets_y {
            let target = Vector3::new(creaking_pos.x, target_y, creaking_pos.z);
            let dir = target - player_eye_pos;
            let distance = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
            if distance > 0.001 {
                let norm_dir = Vector3::new(dir.x / distance, dir.y / distance, dir.z / distance);
                let dot = look_dir.dot(&norm_dir);
                if dot > 1.0 - 0.5 / distance {
                    return true;
                }
            }
        }
        false
    }

    fn is_player_disguised(player: &Player) -> bool {
        use pumpkin_data::tag::{self, Taggable};

        let equipment = player.living_entity.entity_equipment.try_lock();
        if let Ok(equipment) = equipment
            && let Some(head_stack) = equipment.equipment.get(&EquipmentSlot::HEAD)
            && !head_stack.is_empty()
        {
            return head_stack
                .item
                .has_tag(&tag::Item::MINECRAFT_GAZE_DISGUISE_EQUIPMENT);
        }
        false
    }

    pub fn check_can_move(&self) -> bool {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let pos = entity.pos.load();
        let active = self.is_active();

        let players = world.get_nearby_players(pos, 32.0);
        if players.is_empty() {
            if active {
                self.deactivate();
            }
            return true;
        }

        let mut has_potential_target = false;
        for player in players {
            if player.living_entity.entity.is_alive()
                && player.gamemode.load() != pumpkin_util::GameMode::Creative
                && player.gamemode.load() != pumpkin_util::GameMode::Spectator
            {
                has_potential_target = true;
                let is_disguised = Self::is_player_disguised(&player);
                if (!active || !is_disguised) && self.is_looking_at_me(&player) {
                    if active {
                        return false;
                    }
                    let target_pos = player.get_entity().pos.load();
                    let dist_sq = pos.squared_distance_to(target_pos.x, target_pos.y, target_pos.z);
                    if dist_sq < ACTIVATION_RANGE_SQ {
                        self.activate(&player);
                        return false;
                    }
                }
            }
        }

        if !has_potential_target && active {
            self.deactivate();
        }

        true
    }

    pub fn tear_down(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let pos = entity.pos.load();

        world.spawn_particle(
            pos,
            Vector3::new(0.3, 0.3, 0.3),
            0.0,
            100,
            Particle::BlockCrumble,
        );

        self.play_sound(Sound::EntityCreakingDeath);
        entity.remove();
    }

    pub fn creaking_death_effects(&self) {
        self.set_tearing_down();
        self.play_sound(Sound::EntityCreakingTwitch);
    }

    fn play_sound(&self, sound: Sound) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        world.play_sound_fine(sound, SoundCategory::Hostile, &entity.pos.load(), 1.0, 1.0);
    }
}

impl Mob for CreakingEntity {
    /// Vanilla `getAmbientSound`: silent while active.
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        (!self.is_active()).then_some(pumpkin_data::sound::Sound::EntityCreakingAmbient)
    }

    fn mob_is_pushable(&self) -> bool {
        self.mob_entity.living_entity.is_pushable() && self.can_move()
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntityCreakingStep, 0.15, 1.0);
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(pos) = self.get_home_pos() {
            let mut sub = NbtCompound::new();
            sub.put_int("x", pos.0.x);
            sub.put_int("y", pos.0.y);
            sub.put_int("z", pos.0.z);
            nbt.put_compound("home_pos", sub);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(sub) = nbt.get_compound("home_pos")
            && let (Some(x), Some(y), Some(z)) =
                (sub.get_int("x"), sub.get_int("y"), sub.get_int("z"))
        {
            let pos = BlockPos::new(x, y, z);
            self.set_transient(pos);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
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

    /// Vanilla `CreakingAi.updateActivity`: a frozen creaking falls back to its default.
    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        if self.can_move() {
            tick.state
                .set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
        } else {
            tick.state.use_default_activity();
        }
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        // Vanilla `getTarget` reads the brain's attack target.
        let target = self
            .mob_entity
            .with_brain(self, |brain| brain.get(types::ATTACK_TARGET).cloned());
        self.mob_entity.set_target(target);
    }

    /// Vanilla `Creaking.doHurtTarget`.
    fn do_hurt_target(&self, target: &dyn EntityBase) -> bool {
        self.attack_animation_remaining_ticks
            .store(ATTACK_ANIMATION_DURATION, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.world.load().send_entity_status(
            entity,
            EntityStatus::StartAttacking,
            ActorEventID::StartAttacking,
        );
        self.mob_entity.try_attack(entity, target)
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        if !entity.is_alive() {
            return;
        }

        if self
            .invulnerability_animation_remaining_ticks
            .load(Ordering::Relaxed)
            > 0
        {
            self.invulnerability_animation_remaining_ticks
                .fetch_sub(1, Ordering::Relaxed);
        }
        if self
            .attack_animation_remaining_ticks
            .load(Ordering::Relaxed)
            > 0
        {
            self.attack_animation_remaining_ticks
                .fetch_sub(1, Ordering::Relaxed);
        }

        let can_move = self.can_move();
        let now_can_move = self.check_can_move();

        if now_can_move != can_move {
            let world = entity.world.load();
            if now_can_move {
                world.play_sound_fine(
                    Sound::EntityCreakingUnfreeze,
                    SoundCategory::Hostile,
                    &entity.pos.load(),
                    1.0,
                    1.0,
                );
            } else {
                self.stop_in_place();
                world.play_sound_fine(
                    Sound::EntityCreakingFreeze,
                    SoundCategory::Hostile,
                    &entity.pos.load(),
                    1.0,
                    1.0,
                );
            }
            self.set_can_move(now_can_move);
        }

        // Home Creaking Heart check
        if let Some(home_pos) = self.get_home_pos() {
            let world = entity.world.load();
            let has_protection = world.get_block_entity(&home_pos).is_some_and(|be| {
                be.as_any()
                    .downcast_ref::<CreakingHeartBlockEntity>()
                    .is_some_and(|heart_be| heart_be.is_protector(entity.entity_uuid))
            });

            if !has_protection {
                self.mob_entity.living_entity.health.store(0.0);
            }
        }

        // Teardown / death tick
        if self.is_heart_bound() && self.is_tearing_down() {
            let death_time = self.death_time.fetch_add(1, Ordering::Relaxed) + 1;
            if death_time > TWITCH_DEATH_DURATION && !entity.is_removed() {
                self.tear_down();
            }
        }
    }

    fn pre_damage(&self, damage_type: DamageType, _source: Option<&dyn EntityBase>) -> bool {
        let entity = &self.mob_entity.living_entity.entity;

        if self.is_heart_bound() && damage_type != DamageType::OUT_OF_WORLD {
            if self
                .invulnerability_animation_remaining_ticks
                .load(Ordering::Relaxed)
                <= 0
                && entity.is_alive()
            {
                self.invulnerability_animation_remaining_ticks
                    .store(8, Ordering::Relaxed);

                let world = entity.world.load();
                world.broadcast_to_chunk(
                    entity.chunk_pos.load(),
                    &CEntityStatus::new(entity.entity_id, 66),
                );

                if let Some(home_pos) = self.get_home_pos()
                    && let Some(be) = world.get_block_entity(&home_pos)
                    && let Some(heart_be) = be.as_any().downcast_ref::<CreakingHeartBlockEntity>()
                    && heart_be.is_protector(entity.entity_uuid)
                {
                    heart_be.creaking_hurt(&world);
                    self.play_sound(Sound::EntityCreakingSway);
                }
            }
            // Return false so heart-bound Creaking does not lose health from normal damage
            return false;
        }
        true
    }
}

fn creaking<'a>(tick: &BrainTick<'a>) -> Option<&'a CreakingEntity> {
    tick.mob.cast_any().downcast_ref::<CreakingEntity>()
}

fn can_move(tick: &BrainTick<'_>) -> bool {
    creaking(tick).is_some_and(CreakingEntity::can_move)
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(CreakingSwim(Swim::new(0.8)))),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
            ],
        ),
        ActivityData::with_priority_start(
            Activity::Idle,
            10,
            vec![
                Box::new(start_attacking::create(
                    |tick| creaking(tick).is_some_and(CreakingEntity::is_active),
                    |tick| {
                        tick.state
                            .get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
                            .map(|player| player.clone() as Arc<dyn EntityBase>)
                    },
                )),
                Box::new(set_entity_look_target_sometimes::create(8.0, 30, 60)),
                Box::new(run_one(vec![
                    (Box::new(random_stroll::stroll(0.3)), 2),
                    (
                        Box::new(set_walk_target_from_look_target::create(0.3, 3)),
                        2,
                    ),
                    (Box::new(DoNothing::new(30, 60)), 1),
                ])),
            ],
        ),
        ActivityData::with_gate_memory(
            Activity::Fight,
            10,
            vec![
                Box::new(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
                Box::new(melee_attack::create_if(can_move, 40)),
                Box::new(stop_attacking_if_target_invalid::create_with(
                    |tick, target| !is_attack_target_still_reachable(tick, target),
                    |_, _| {},
                    true,
                )),
            ],
            types::ATTACK_TARGET.id(),
        ),
    ]
}

/// Vanilla `CreakingAi.isAttackTargetStillReachable`: a player it can still see and attack.
fn is_attack_target_still_reachable(tick: &BrainTick<'_>, target: &Arc<dyn EntityBase>) -> bool {
    let id = target.get_entity().entity_id;
    target.get_player().is_some()
        && tick
            .state
            .get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYERS)
            .is_some_and(|players| {
                players
                    .iter()
                    .any(|player| player.get_entity().entity_id == id)
            })
}

/// Vanilla `CreakingAi`'s `Swim` subclass: no swimming while frozen.
struct CreakingSwim(Swim);

impl Behavior for CreakingSwim {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        self.0.entry_conditions()
    }

    fn min_duration(&self) -> i32 {
        self.0.min_duration()
    }

    fn max_duration(&self) -> i32 {
        self.0.max_duration()
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        can_move(tick) && self.0.check_extra_start_conditions(tick)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        self.0.start(tick);
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        self.0.tick(tick);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        self.0.stop(tick);
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        self.0.can_still_use(tick)
    }

    fn debug_name(&self) -> &'static str {
        "Swim"
    }
}
