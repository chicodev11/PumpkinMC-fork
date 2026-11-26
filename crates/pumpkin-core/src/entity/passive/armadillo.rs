use std::sync::{
    Arc,
    atomic::{AtomicI32, AtomicU64, Ordering},
};

use pumpkin_data::BlockState;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::position::BlockPos;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::brain::behavior::utils,
    ai::brain::{
        Brain, BrainProvider, BrainTick,
        activity::ActivityData,
        behavior::{
            AnimalMakeLove, AnimalPanic, Behavior, CountDownCooldownTicks, DoNothing,
            FollowTemptation, LookAtTargetSink, MoveToTargetSink, OneShot, RandomLookAround, Swim,
            Timed, baby_follow_adult, random_stroll, run_one, run_one_with_conditions,
            set_entity_look_target_sometimes, set_walk_target_from_look_target,
        },
        memory::{MemoryModuleId, MemoryStatus, PackedMemories, types},
        sensing::SensorType,
    },
    item::ItemEntity,
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

const ARMADILLO_BRUSH_LOOT_TABLE: &str = "minecraft:brush/armadillo";
pub const ARMADILLO_BABY_START_AGE: i32 = -48000;
/// Vanilla `Armadillo.SCARE_CHECK_INTERVAL`: how long `DANGER_DETECTED_RECENTLY` lasts.
pub const SCARE_CHECK_INTERVAL: i64 = 80;
pub const SCARE_DISTANCE_HORIZONTAL: f64 = 7.0;
pub const SCARE_DISTANCE_VERTICAL: f64 = 2.0;
const SHED_LOOT_TABLE: &str = "minecraft:gameplay/armadillo_shed";

const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 2.0;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f32 = 1.0;
const DEFAULT_CLOSE_ENOUGH_DIST: f64 = 2.0;
const BABY_CLOSE_ENOUGH_DIST: f64 = 1.0;
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);

fn pick_next_scute_drop_time() -> i32 {
    let rand_ticks = (rand::random::<u32>() % 6000) as i32;
    rand_ticks + 6000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum ArmadilloState {
    #[default]
    Idle = 0,
    Rolling = 1,
    Scared = 2,
    Unrolling = 3,
}

impl ArmadilloState {
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Rolling,
            2 => Self::Scared,
            3 => Self::Unrolling,
            _ => Self::Idle,
        }
    }

    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub const fn is_threatened(self) -> bool {
        !matches!(self, Self::Idle)
    }

    #[must_use]
    pub const fn animation_duration(self) -> u64 {
        match self {
            Self::Idle => 0,
            Self::Rolling => 10,
            Self::Scared => 50,
            Self::Unrolling => 30,
        }
    }

    #[must_use]
    pub const fn should_hide_in_shell(self, ticks_in_state: u64) -> bool {
        match self {
            Self::Idle => false,
            Self::Rolling => ticks_in_state > 5,
            Self::Scared => true,
            Self::Unrolling => ticks_in_state < 26,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Rolling => "rolling",
            Self::Scared => "scared",
            Self::Unrolling => "unrolling",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "rolling" => Self::Rolling,
            "scared" => Self::Scared,
            "unrolling" => Self::Unrolling,
            _ => Self::Idle,
        }
    }
}

/// Represents an Armadillo, a passive entity that can roll into a ball when threatened.
///
/// Wiki: <https://minecraft.wiki/w/Armadillo>
pub struct ArmadilloEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub state: AtomicI32,
    pub in_state_ticks: AtomicU64,
    pub scute_time: AtomicI32,
}

impl ArmadilloEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        Arc::new(Self {
            mob_entity: MobEntity::new(entity),
            ageable_data: AgeableData::default(),
            state: AtomicI32::new(ArmadilloState::Idle.id()),
            in_state_ticks: AtomicU64::new(0),
            scute_time: AtomicI32::new(pick_next_scute_drop_time()),
        })
    }

    #[must_use]
    pub fn get_state(&self) -> ArmadilloState {
        ArmadilloState::from_id(self.state.load(Ordering::Relaxed))
    }

    pub fn switch_to_state(&self, state: ArmadilloState) {
        self.state.store(state.id(), Ordering::Relaxed);
        self.in_state_ticks.store(0, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::armadillo::ARMADILLO_STATE,
            VarInt(state.id()),
        );
    }

    #[must_use]
    pub fn is_scared(&self) -> bool {
        self.get_state() != ArmadilloState::Idle
    }

    #[must_use]
    pub fn should_hide_in_shell(&self) -> bool {
        self.get_state()
            .should_hide_in_shell(self.in_state_ticks.load(Ordering::Relaxed))
    }

    #[must_use]
    pub fn should_switch_to_scared_state(&self) -> bool {
        self.get_state() == ArmadilloState::Rolling
            && self.in_state_ticks.load(Ordering::Relaxed)
                > ArmadilloState::Rolling.animation_duration()
    }

    pub fn can_stay_rolled_up(&self) -> bool {
        !self.is_panicking()
            && !self.mob_entity.living_entity.is_in_water()
            && !self.get_entity().has_vehicle()
    }

    pub fn roll_up(&self) {
        if !self.is_scared() {
            self.mob_entity.reset_love_ticks();
            let entity = self.get_entity();
            let world = entity.world.load();
            world.play_sound(
                Sound::EntityArmadilloRoll,
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
            self.switch_to_state(ArmadilloState::Rolling);
        }
    }

    pub fn roll_out(&self) {
        if self.is_scared() {
            let entity = self.get_entity();
            let world = entity.world.load();
            world.play_sound(
                Sound::EntityArmadilloUnrollFinish,
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
            self.switch_to_state(ArmadilloState::Idle);
        }
    }

    pub fn brush_off_scute(
        &self,
        _interacting_entity: Option<&dyn EntityBase>,
        tool: &ItemStack,
    ) -> bool {
        if self.is_baby() || !self.get_entity().is_alive() {
            return false;
        }
        let entity = self.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();
        if let Some(table) = world.get_loot_table(ARMADILLO_BRUSH_LOOT_TABLE) {
            let params = crate::world::loot::LootContextParameters {
                world: Some(&world),
                position: Some(pos),
                this_entity: Some(self),
                tool: Some(tool.clone()),
                ..Default::default()
            };
            for drop_stack in
                crate::world::loot::generate_loot_from_handle(&table, rand::random(), &params)
            {
                let item_entity = Arc::new(ItemEntity::new(
                    Entity::new(world.clone(), pos, &EntityType::ITEM),
                    drop_stack,
                ));
                world.spawn_entity(item_entity);
            }
        }
        world.play_sound(Sound::EntityArmadilloBrush, SoundCategory::Neutral, &pos);
        world.emit_game_event(
            pumpkin_data::game_event::GameEvent::EntityInteract.name(),
            pos,
        );
        true
    }

    /// Vanilla `Armadillo.isScaredBy`.
    pub fn is_scared_by(&self, target: &dyn EntityBase) -> bool {
        let scare_box = self.get_entity().bounding_box.load().expand(
            SCARE_DISTANCE_HORIZONTAL,
            SCARE_DISTANCE_VERTICAL,
            SCARE_DISTANCE_HORIZONTAL,
        );
        let target_entity = target.get_entity();
        if !target_entity.bounding_box.load().intersects(&scare_box) {
            return false;
        }
        if target_entity
            .entity_type
            .has_tag(&tag::EntityType::MINECRAFT_UNDEAD)
        {
            return true;
        }
        if self
            .mob_entity
            .living_entity
            .get_last_hurt_by_mob_id()
            .is_some_and(|id| id == target_entity.entity_id)
        {
            return true;
        }
        target.get_player().is_some()
            && !target.is_spectator()
            && (target_entity.is_sprinting() || target_entity.has_vehicle())
    }

    /// Vanilla `customServerAiStep`'s shedding: a scute from the shed loot table.
    fn tick_scute_shedding(&self) {
        let entity = self.get_entity();
        if !utils::is_alive(self) || self.is_baby() {
            return;
        }
        if self.scute_time.fetch_sub(1, Ordering::Relaxed) - 1 > 0 {
            return;
        }
        let world = entity.world.load();
        if let Some(table) = world.get_loot_table(SHED_LOOT_TABLE) {
            let params = crate::world::loot::LootContextParameters {
                this_entity: Some(self),
                ..Default::default()
            };
            let drops =
                crate::world::loot::generate_loot_from_handle(&table, rand::random(), &params);
            if !drops.is_empty() {
                let mut rng = self.get_random();
                let pitch = (rng.random::<f32>() - rng.random::<f32>()).mul_add(0.2, 1.0);
                self.play_entity_sound(Sound::EntityArmadilloScuteDrop, 1.0, pitch);
                for stack in drops {
                    entity.spawn_at_location(stack);
                }
                world.emit_game_event(GameEvent::EntityPlace.name(), entity.pos.load());
            }
        }
        self.scute_time
            .store(pick_next_scute_drop_time(), Ordering::Relaxed);
    }
}

impl AgeableMob for ArmadilloEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    fn get_baby_start_age(&self) -> i32 {
        ARMADILLO_BABY_START_AGE
    }
}

impl Animal for ArmadilloEntity {
    fn play_eating_sound(&self) {
        self.make_sound(Sound::EntityArmadilloEat);
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack
            .item
            .has_tag(&tag::Item::MINECRAFT_ARMADILLO_FOOD)
    }
}

impl Mob for ArmadilloEntity {
    /// Vanilla `getAmbientSound`: silent while rolled up.
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        (!self.is_scared()).then_some(pumpkin_data::sound::Sound::EntityArmadilloAmbient)
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntityArmadilloStep, 0.15, 1.0);
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_string("state", self.get_state().name().to_string());
        nbt.put_int("scute_time", self.scute_time.load(Ordering::Relaxed));
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(state_name) = nbt.get_string("state") {
            self.switch_to_state(ArmadilloState::from_name(state_name));
        } else if let Some(state_id) = nbt.get_int("state") {
            self.switch_to_state(ArmadilloState::from_id(state_id));
        }
        if let Some(scute_time) = nbt.get_int("scute_time") {
            self.scute_time.store(scute_time, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn modify_incoming_damage(&self, amount: f32, _damage_type: DamageType) -> f32 {
        if self.is_scared() {
            (amount - 1.0).max(0.0) / 2.0
        } else {
            amount
        }
    }

    /// Vanilla `Armadillo.actuallyHurt`: a living attacker scares it into a ball; an
    /// environmental panic cause makes it unroll and run.
    fn on_damage_with_context(
        &self,
        damage_type: DamageType,
        _source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        if self.mob_entity.is_no_ai() || !utils::is_alive(self) {
            return true;
        }
        if cause.is_some_and(|cause| cause.get_living_entity().is_some()) {
            self.mob_entity.post_to_brain(Box::new(|tick| {
                tick.state.set_with_expiry(
                    types::DANGER_DETECTED_RECENTLY,
                    true,
                    SCARE_CHECK_INTERVAL,
                );
            }));
            if self.can_stay_rolled_up() {
                self.roll_up();
            }
        } else if damage_type.has_tag(&tag::DamageType::MINECRAFT_PANIC_ENVIRONMENTAL_CAUSES) {
            self.roll_out();
        }
        true
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.in_state_ticks.fetch_add(1, Ordering::Relaxed);
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        BrainProvider {
            memory_types: &[],
            sensor_types: &[
                SensorType::NearestLivingEntities,
                SensorType::HurtBy,
                SensorType::FoodTemptations,
                SensorType::NearestAdult,
                SensorType::ArmadilloScareDetected,
            ],
            activities,
        }
        .make_brain(self, packed)
    }

    /// Vanilla `ArmadilloAi.updateActivity`.
    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        tick.state
            .set_active_activity_to_first_valid(&[Activity::Panic, Activity::Idle]);
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        self.tick_scute_shedding();
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::armadillo::BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::armadillo::ARMADILLO_STATE,
            VarInt(self.get_state().id()),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if item_stack.item == &Item::BRUSH
            && self.brush_off_scute(Some(player.as_ref()), item_stack)
        {
            if player.gamemode.load() != pumpkin_util::GameMode::Creative {
                let _ = item_stack.damage_item(16);
            }
            return true;
        }
        if self.is_scared() {
            return false;
        }
        self.animal_interact(player, item_stack)
    }
}

fn armadillo<'a>(tick: &BrainTick<'a>) -> Option<&'a ArmadilloEntity> {
    tick.mob.cast_any().downcast_ref::<ArmadilloEntity>()
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(Swim::new(0.8))),
                Box::new(Timed::new(ArmadilloPanic(
                    AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING)
                        .with_panic_causes(&tag::DamageType::MINECRAFT_PANIC_ENVIRONMENTAL_CAUSES),
                ))),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(ArmadilloMoveToTargetSink(
                    MoveToTargetSink::default(),
                ))),
                Box::new(CountDownCooldownTicks::new(
                    types::TEMPTATION_COOLDOWN_TICKS,
                )),
                Box::new(CountDownCooldownTicks::new(types::GAZE_COOLDOWN_TICKS)),
                Box::new(rolling_out()),
            ],
        ),
        ActivityData::with_priority_start(
            Activity::Idle,
            0,
            vec![
                Box::new(set_entity_look_target_sometimes::create_matching(
                    Some(&EntityType::PLAYER),
                    6.0,
                    30,
                    60,
                )),
                Box::new(Timed::new(
                    AnimalMakeLove::new(&EntityType::ARMADILLO)
                        .with_speed(SPEED_MULTIPLIER_WHEN_MAKING_LOVE, 1),
                )),
                Box::new(run_one(vec![
                    (
                        Box::new(Timed::new(FollowTemptation::new(
                            |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                            |mob| {
                                if mob.get_mob_entity().living_entity.is_baby() {
                                    BABY_CLOSE_ENOUGH_DIST
                                } else {
                                    DEFAULT_CLOSE_ENOUGH_DIST
                                }
                            },
                        ))),
                        1,
                    ),
                    (
                        Box::new(baby_follow_adult::create(
                            ADULT_FOLLOW_RANGE.0,
                            ADULT_FOLLOW_RANGE.1,
                            SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                        )),
                        1,
                    ),
                ])),
                Box::new(Timed::new(RandomLookAround::new(150, 250, 30.0, 0.0, 0.0))),
                Box::new(run_one_with_conditions(
                    vec![(types::WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
                    vec![
                        (
                            Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                            1,
                        ),
                        (
                            Box::new(set_walk_target_from_look_target::create(
                                SPEED_MULTIPLIER_WHEN_IDLING,
                                3,
                            )),
                            1,
                        ),
                        (Box::new(DoNothing::new(30, 60)), 1),
                    ],
                )),
            ],
        ),
        ActivityData::with_priority_start_and_conditions(
            Activity::Panic,
            0,
            vec![Box::new(Timed::new(ArmadilloBallUp::default()))],
            vec![
                (
                    types::DANGER_DETECTED_RECENTLY.id(),
                    MemoryStatus::ValuePresent,
                ),
                (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
            ],
        ),
    ]
}

/// Vanilla `ArmadilloAi.ARMADILLO_ROLLING_OUT`: unrolls once the danger memory is gone.
fn rolling_out() -> OneShot {
    OneShot::new(
        "ArmadilloRollingOut",
        vec![(
            types::DANGER_DETECTED_RECENTLY.id(),
            MemoryStatus::ValueAbsent,
        )],
        |tick| {
            let Some(armadillo) = armadillo(tick) else {
                return false;
            };
            if !armadillo.is_scared() {
                return false;
            }
            armadillo.roll_out();
            true
        },
    )
}

/// Vanilla `ArmadilloAi.ArmadilloPanic`: unrolls before running.
struct ArmadilloPanic(AnimalPanic);

impl Behavior for ArmadilloPanic {
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
        self.0.check_extra_start_conditions(tick)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(armadillo) = armadillo(tick) {
            armadillo.roll_out();
        }
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
        "ArmadilloPanic"
    }
}

/// Vanilla `ArmadilloAi`'s `MoveToTargetSink` subclass: no walking while rolled up.
struct ArmadilloMoveToTargetSink(MoveToTargetSink);

impl Behavior for ArmadilloMoveToTargetSink {
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
        !armadillo(tick).is_some_and(ArmadilloEntity::is_scared)
            && self.0.check_extra_start_conditions(tick)
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
        "MoveToTargetSink"
    }
}

/// Vanilla `ArmadilloAi.ArmadilloBallUp`: stays rolled up while danger lasts, peeking out now
/// and then, and starts unrolling once the danger memory is about to run out.
#[derive(Default)]
struct ArmadilloBallUp {
    next_peek_timer: i64,
    danger_was_around: bool,
}

impl ArmadilloBallUp {
    /// Vanilla `BALL_UP_STAY_IN_STATE`: five minutes.
    const BALL_UP_STAY_IN_STATE: i32 = 5 * 60 * 20;
    /// Vanilla `DANGER_DETECTED_RECENTLY_DANGER_THRESHOLD`.
    const DANGER_THRESHOLD: i64 = 75;

    fn pick_next_peek_timer(armadillo: &ArmadilloEntity) -> i64 {
        ArmadilloState::Scared.animation_duration() as i64
            + armadillo.get_random().random_range(100..=400)
    }
}

impl Behavior for ArmadilloBallUp {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn min_duration(&self) -> i32 {
        Self::BALL_UP_STAY_IN_STATE
    }

    fn max_duration(&self) -> i32 {
        Self::BALL_UP_STAY_IN_STATE
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let entity = tick.mob.get_entity();
        entity.on_ground.load(Ordering::Relaxed) && !entity.is_in_liquid()
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        armadillo(tick).is_some_and(|armadillo| armadillo.get_state().is_threatened())
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(armadillo) = armadillo(tick) {
            armadillo.roll_up();
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(armadillo) = armadillo(tick)
            && !armadillo.can_stay_rolled_up()
        {
            armadillo.roll_out();
        }
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(armadillo) = armadillo(tick) else {
            return;
        };
        if self.next_peek_timer > 0 {
            self.next_peek_timer -= 1;
        }
        let entity = armadillo.get_entity();
        let on_ground = entity.on_ground.load(Ordering::Relaxed);
        if armadillo.should_switch_to_scared_state() {
            armadillo.switch_to_state(ArmadilloState::Scared);
            if on_ground {
                armadillo.play_entity_sound(Sound::EntityArmadilloLand, 1.0, 1.0);
            }
            return;
        }
        let state = armadillo.get_state();
        let danger_left = tick
            .state
            .time_until_expiry(types::DANGER_DETECTED_RECENTLY.id())
            .unwrap_or(0);
        let danger_is_around = danger_left > Self::DANGER_THRESHOLD;
        if danger_is_around != self.danger_was_around {
            self.next_peek_timer = Self::pick_next_peek_timer(armadillo);
        }
        self.danger_was_around = danger_is_around;
        let unrolling = ArmadilloState::Unrolling.animation_duration() as i64;
        if state == ArmadilloState::Scared {
            if self.next_peek_timer == 0 && on_ground && danger_is_around {
                tick.world.send_entity_status(
                    entity,
                    EntityStatus::ArmadilloPeek,
                    ActorEventID::None,
                );
                self.next_peek_timer = Self::pick_next_peek_timer(armadillo);
            }
            if danger_left < unrolling {
                armadillo.play_entity_sound(Sound::EntityArmadilloUnrollStart, 1.0, 1.0);
                armadillo.switch_to_state(ArmadilloState::Unrolling);
            }
        } else if state == ArmadilloState::Unrolling && danger_left > unrolling {
            armadillo.switch_to_state(ArmadilloState::Scared);
        }
    }

    fn debug_name(&self) -> &'static str {
        "ArmadilloBallUp"
    }
}
