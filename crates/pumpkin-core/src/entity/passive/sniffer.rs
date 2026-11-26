use std::sync::{
    Arc,
    atomic::{AtomicI32, Ordering},
};

use pumpkin_data::BlockState;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::boundingbox::EntityDimensions;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::brain::{
        Brain, BrainProvider, BrainState, BrainTick,
        activity::ActivityData,
        behavior::{
            AnimalMakeLove, AnimalPanic, Behavior, CountDownCooldownTicks, DoNothing,
            FollowTemptation, LookAtTargetSink, MoveToTargetSink, Swim, Timed,
            interact::set_entity_look_target, random_stroll, run_one,
            set_walk_target_from_look_target, utils,
        },
        memory::{MemoryModuleId, MemoryStatus, PackedMemories, types, walk_target::WalkTarget},
        sensing::SensorType,
    },
    item::ItemEntity,
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

pub const SNIFFER_BABY_START_AGE: i32 = -48000;
pub const DIGGING_DROP_SEED_OFFSET_TICKS: i32 = 120;
/// Vanilla `SnifferAi.SNIFFING_COOLDOWN_TICKS`: eight minutes between finished digs.
const SNIFFING_COOLDOWN_TICKS: i64 = 9600;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 2.0;
const SPEED_MULTIPLIER_WHEN_SNIFFING: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
const DIGGING_LOOT_TABLE: &str = "minecraft:gameplay/sniffer_digging";
/// Vanilla `storeExploredPosition` keeps the 20 newest before adding another.
const MAX_EXPLORED_POSITIONS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum SnifferState {
    #[default]
    Idling = 0,
    FeelingHappy = 1,
    Scenting = 2,
    Sniffing = 3,
    Searching = 4,
    Digging = 5,
    Rising = 6,
}

impl SnifferState {
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::FeelingHappy,
            2 => Self::Scenting,
            3 => Self::Sniffing,
            4 => Self::Searching,
            5 => Self::Digging,
            6 => Self::Rising,
            _ => Self::Idling,
        }
    }

    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }
}

/// Represents a Sniffer, a large passive mob that digs up ancient seeds.
///
/// Wiki: <https://minecraft.wiki/w/Sniffer>
pub struct SnifferEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub state: AtomicI32,
    pub drop_seed_at_tick: AtomicI32,
}

impl SnifferEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        Arc::new(Self {
            mob_entity: MobEntity::new(entity),
            ageable_data: AgeableData::default(),
            state: AtomicI32::new(SnifferState::Idling.id()),
            drop_seed_at_tick: AtomicI32::new(0),
        })
    }

    #[must_use]
    pub fn get_state(&self) -> SnifferState {
        SnifferState::from_id(self.state.load(Ordering::Relaxed))
    }

    pub fn set_state(&self, state: SnifferState) {
        self.state.store(state.id(), Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::sniffer::STATE,
            VarInt(state.id()),
        );
        self.update_dimensions();
    }

    /// Vanilla `Sniffer.getDefaultDimensions`: 0.4 lower while digging, by age otherwise.
    fn update_dimensions(&self) {
        let entity = self.get_entity();
        let baby = entity.age.load(Ordering::Relaxed) < 0;
        let dimensions = if self.get_state() == SnifferState::Digging {
            let size = Entity::type_dimensions(&EntityType::SNIFFER);
            let digging = EntityDimensions::new(size.width, size.height - 0.4, 0.81);
            if baby { digging.scale(0.5) } else { digging }
        } else {
            Entity::age_dimensions(&EntityType::SNIFFER, baby)
        };
        entity.set_default_dimensions(dimensions);
    }

    pub fn transition_to(&self, state: SnifferState) {
        let entity = self.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();

        match state {
            SnifferState::Idling => {
                self.set_state(SnifferState::Idling);
            }
            SnifferState::FeelingHappy => {
                world.play_sound(Sound::EntitySnifferHappy, SoundCategory::Neutral, &pos);
                self.set_state(SnifferState::FeelingHappy);
            }
            SnifferState::Scenting => {
                self.set_state(SnifferState::Scenting);
                self.on_scenting_start();
            }
            SnifferState::Sniffing => {
                world.play_sound(Sound::EntitySnifferSniffing, SoundCategory::Neutral, &pos);
                self.set_state(SnifferState::Sniffing);
            }
            SnifferState::Searching => {
                self.set_state(SnifferState::Searching);
            }
            SnifferState::Digging => {
                self.set_state(SnifferState::Digging);
                self.on_digging_start();
            }
            SnifferState::Rising => {
                world.play_sound(
                    Sound::EntitySnifferDiggingStop,
                    SoundCategory::Neutral,
                    &pos,
                );
                self.set_state(SnifferState::Rising);
            }
        }
    }

    fn on_scenting_start(&self) {
        let entity = self.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();
        let pitch = if self.is_baby() { 1.3 } else { 1.0 };
        world.play_sound_fine(
            Sound::EntitySnifferScenting,
            SoundCategory::Neutral,
            &pos,
            1.0,
            pitch,
        );
    }

    fn on_digging_start(&self) {
        let entity = self.get_entity();
        let world = entity.world.load();
        // Vanilla counts from the sniffer's own `tickCount`, which the client also animates by.
        let drop_tick = entity.tick_count.load(Ordering::Relaxed) + DIGGING_DROP_SEED_OFFSET_TICKS;
        self.drop_seed_at_tick.store(drop_tick, Ordering::Relaxed);
        entity.set_synced_data(
            pumpkin_data::tracked_data::sniffer::DROP_SEED_AT_TICK,
            VarInt(drop_tick),
        );
        world.send_entity_status(
            entity,
            EntityStatus::SnifferDiggingSound,
            ActorEventID::None,
        );
    }

    /// Vanilla `onDiggingComplete`: a dig that ran its course marks the spot explored.
    pub fn on_digging_complete(&self, brain: &mut BrainState, success: bool) {
        if success {
            self.store_explored_position(brain, self.get_entity().get_on_pos());
        }
    }

    #[must_use]
    pub fn get_head_position(&self) -> Vector3<f64> {
        let entity = self.get_entity();
        let pos = entity.pos.load();
        let yaw = f64::from(entity.yaw.load());
        let yaw_rad = yaw.to_radians();
        let forward = Vector3::new(-yaw_rad.sin(), 0.0, yaw_rad.cos());
        pos + forward * 2.25
    }

    #[must_use]
    pub fn get_head_block(&self) -> BlockPos {
        let head_pos = self.get_head_position();
        let entity = self.get_entity();
        let pos = entity.pos.load();
        BlockPos::floored(head_pos.x, pos.y + 0.2, head_pos.z)
    }

    /// Vanilla `isTempted`.
    fn is_tempted(brain: &BrainState) -> bool {
        brain.has_memory_value(types::TEMPTING_PLAYER.id())
    }

    /// Vanilla `canSniff`.
    #[must_use]
    pub fn can_sniff(&self, brain: &BrainState) -> bool {
        let entity = self.get_entity();
        !Self::is_tempted(brain)
            && !brain.has_memory_value(types::IS_PANICKING.id())
            && !entity.is_in_water()
            && !self.mob_entity.is_in_love()
            && entity.on_ground.load(Ordering::Relaxed)
            && !entity.has_vehicle()
            && !entity.is_leashed()
    }

    /// Vanilla `canDig()`.
    #[must_use]
    pub fn can_dig(&self, brain: &BrainState) -> bool {
        let entity = self.get_entity();
        !brain.has_memory_value(types::IS_PANICKING.id())
            && !Self::is_tempted(brain)
            && !self.is_baby()
            && !entity.is_in_water()
            && entity.on_ground.load(Ordering::Relaxed)
            && !entity.has_vehicle()
            && self.can_dig_at(brain, self.get_head_block().down())
    }

    /// Vanilla `canDig(BlockPos)`: diggable, not explored yet, and reachable.
    fn can_dig_at(&self, brain: &BrainState, pos: BlockPos) -> bool {
        let entity = self.get_entity();
        let world = entity.world.load();
        if !pumpkin_data::Block::from_state_id(world.get_block_state_id(&pos))
            .has_tag(&tag::Block::MINECRAFT_SNIFFER_DIGGABLE_BLOCK)
        {
            return false;
        }
        let explored = brain
            .get(types::SNIFFER_EXPLORED_POSITIONS)
            .is_some_and(|explored| {
                utils::global_pos_in(&world, pos).is_some_and(|here| explored.contains(&here))
            });
        if explored {
            return false;
        }
        let steered = self.mob_entity.steered();
        steered
            .navigator()
            .create_path(&steered.mob_entity().living_entity, pos.0.to_f64(), 1)
            .is_some_and(|path| path.can_reach())
    }

    /// Vanilla `calculateDigPosition`: up to five stroll targets, each a bit further out.
    fn calculate_dig_position(&self, brain: &BrainState) -> Option<BlockPos> {
        let world = self.get_entity().world.load();
        (0..5)
            .filter_map(|i| crate::entity::ai::util::land_random_pos::get_pos(self, 10 + 2 * i, 3))
            .map(|pos| pos.to_block_pos())
            .filter(|pos| {
                world
                    .worldborder
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains_block(pos.0.x, pos.0.z)
            })
            .map(|pos| pos.down())
            .find(|pos| self.can_dig_at(brain, *pos))
    }

    /// Vanilla `storeExploredPosition`.
    fn store_explored_position(&self, brain: &mut BrainState, pos: BlockPos) {
        let world = self.get_entity().world.load();
        let Some(here) = utils::global_pos_in(&world, pos) else {
            return;
        };
        let mut explored: Vec<_> = brain
            .get(types::SNIFFER_EXPLORED_POSITIONS)
            .map(|explored| {
                explored
                    .iter()
                    .take(MAX_EXPLORED_POSITIONS)
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        explored.insert(0, here);
        brain.set(types::SNIFFER_EXPLORED_POSITIONS, explored);
    }

    /// Vanilla `dropSeed`: the digging loot drops at the head block on the planned tick.
    pub fn drop_seed(&self) {
        let entity = self.get_entity();
        if self.drop_seed_at_tick.load(Ordering::Relaxed)
            != entity.tick_count.load(Ordering::Relaxed)
        {
            return;
        }
        let world = entity.world.load();
        let head = self.get_head_block();
        if let Some(table) = world.get_loot_table(DIGGING_LOOT_TABLE) {
            let params = crate::world::loot::LootContextParameters {
                this_entity: Some(self),
                ..Default::default()
            };
            for stack in
                crate::world::loot::generate_loot_from_handle(&table, rand::random(), &params)
            {
                let item = ItemEntity::new(
                    Entity::new(world.clone(), head.0.to_f64(), &EntityType::ITEM),
                    stack,
                );
                world.spawn_entity(Arc::new(item));
            }
        }
        self.play_entity_sound(Sound::EntitySnifferDropSeed, 1.0, 1.0);
    }

    /// Vanilla `resetSniffing`.
    fn reset_sniffing(&self, brain: &mut BrainState) {
        brain.erase(types::SNIFFER_DIGGING.id());
        brain.erase(types::SNIFFER_SNIFFING_TARGET.id());
        self.transition_to(SnifferState::Idling);
    }
}

impl AgeableMob for SnifferEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    fn get_baby_start_age(&self) -> i32 {
        SNIFFER_BABY_START_AGE
    }

    fn age_boundary_reached(&self) {
        self.update_dimensions();
    }
}

impl Animal for SnifferEntity {
    fn play_eating_sound(&self) {
        let pitch = 0.8 + self.get_random().random::<f32>() * 0.4;
        self.play_entity_sound(Sound::EntitySnifferEat, 1.0, pitch);
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_SNIFFER_FOOD)
    }

    /// Vanilla `Sniffer.spawnChildFromBreeding` lays an egg instead of a baby.
    fn spawns_offspring(&self) -> bool {
        false
    }

    fn on_bred_without_offspring(&self, _brain: Option<&mut BrainState>) {
        let entity = self.get_entity();
        let world = entity.world.load();
        let egg = ItemEntity::new(
            Entity::new(world.clone(), entity.pos.load(), &EntityType::ITEM),
            ItemStack::new(1, &Item::SNIFFER_EGG),
        );
        let mut rng = self.get_random();
        let pitch = (rng.random::<f32>() - rng.random::<f32>()).mul_add(0.2, 0.5);
        self.play_entity_sound(Sound::BlockSnifferEggPlop, 1.0, pitch);
        world.spawn_entity(Arc::new(egg));
    }
}

impl Mob for SnifferEntity {
    /// Vanilla `getAmbientSound`: silent while digging or searching.
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        (!matches!(
            self.get_state(),
            SnifferState::Digging | SnifferState::Searching
        ))
        .then_some(pumpkin_data::sound::Sound::EntitySnifferIdle)
    }

    fn mob_support_quad_leash(&self) -> bool {
        true
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntitySnifferStep, 0.15, 1.0);
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int("State", self.get_state().id());
        nbt.put_int(
            "DropSeedAtTick",
            self.drop_seed_at_tick.load(Ordering::Relaxed),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(state_id) = nbt.get_int("State") {
            self.set_state(SnifferState::from_id(state_id));
        }
        if let Some(drop_tick) = nbt.get_int("DropSeedAtTick") {
            self.drop_seed_at_tick.store(drop_tick, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    /// Vanilla `Sniffer.tick`; the searching sound and digging particles are client side.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        if self.get_state() == SnifferState::Digging {
            self.drop_seed();
        }
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        BrainProvider {
            memory_types: &[],
            sensor_types: &[
                SensorType::NearestLivingEntities,
                SensorType::HurtBy,
                SensorType::NearestPlayers,
                SensorType::FoodTemptations,
            ],
            activities,
        }
        .make_brain(self, packed)
    }

    /// Vanilla `SnifferAi.updateActivity`.
    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        tick.state.set_active_activity_to_first_valid(&[
            Activity::Dig,
            Activity::Sniff,
            Activity::Idle,
        ]);
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::sniffer::BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::sniffer::STATE,
            VarInt(self.get_state().id()),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::sniffer::DROP_SEED_AT_TICK,
            VarInt(self.drop_seed_at_tick.load(Ordering::Relaxed)),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.animal_interact(player, item_stack)
    }
}

fn sniffer<'a>(tick: &BrainTick<'a>) -> Option<&'a SnifferEntity> {
    tick.mob.cast_any().downcast_ref::<SnifferEntity>()
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(Swim::new(0.8))),
                Box::new(Timed::new(ResetSniffingOnStart(AnimalPanic::new(
                    SPEED_MULTIPLIER_WHEN_PANICKING,
                )))),
                Box::new(Timed::new(MoveToTargetSink::new(500, 700))),
                Box::new(CountDownCooldownTicks::new(
                    types::TEMPTATION_COOLDOWN_TICKS,
                )),
            ],
        ),
        ActivityData::with_priority_start_and_conditions(
            Activity::Idle,
            0,
            vec![
                Box::new(Timed::new(ResetSniffingOnStart(AnimalMakeLove::new(
                    &EntityType::SNIFFER,
                )))),
                Box::new(Timed::new(ResetSniffingOnStart(FollowTemptation::new(
                    |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                    |mob| {
                        if mob.get_mob_entity().living_entity.is_baby() {
                            2.5
                        } else {
                            3.5
                        }
                    },
                )))),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(FeelingHappy)),
                Box::new(run_one(vec![
                    (
                        Box::new(set_walk_target_from_look_target::create(
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            3,
                        )),
                        2,
                    ),
                    (Box::new(Timed::new(Scenting)), 1),
                    (Box::new(Timed::new(Sniffing)), 1),
                    (
                        Box::new(set_entity_look_target::for_type(&EntityType::PLAYER, 6.0)),
                        1,
                    ),
                    (
                        Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                        1,
                    ),
                    (Box::new(DoNothing::new(5, 20)), 2),
                ])),
            ],
            vec![(types::SNIFFER_DIGGING.id(), MemoryStatus::ValueAbsent)],
        ),
        ActivityData::with_priority_start_and_conditions(
            Activity::Sniff,
            0,
            vec![Box::new(Timed::new(Searching))],
            vec![
                (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
                (
                    types::SNIFFER_SNIFFING_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
                (types::WALK_TARGET.id(), MemoryStatus::ValuePresent),
            ],
        ),
        ActivityData::with_pairs_and_conditions(
            Activity::Dig,
            vec![
                (0, Box::new(Timed::new(Digging))),
                (0, Box::new(Timed::new(FinishedDigging))),
            ],
            vec![
                (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
                (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
                (types::SNIFFER_DIGGING.id(), MemoryStatus::ValuePresent),
            ],
        ),
    ]
}

/// Vanilla's anonymous `SnifferAi` subclasses of `AnimalPanic`, `AnimalMakeLove` and
/// `FollowTemptation`: each drops any sniffing in progress before it starts.
struct ResetSniffingOnStart<B>(B);

impl<B: Behavior> Behavior for ResetSniffingOnStart<B> {
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
        if let Some(sniffer) = sniffer(tick) {
            sniffer.reset_sniffing(tick.state);
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
        self.0.debug_name()
    }
}

const SCENTING_CONDITIONS: [(MemoryModuleId, MemoryStatus); 5] = [
    (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
    (types::SNIFFER_DIGGING.id(), MemoryStatus::ValueAbsent),
    (
        types::SNIFFER_SNIFFING_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (types::SNIFFER_HAPPY.id(), MemoryStatus::ValueAbsent),
    (types::BREED_TARGET.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `SnifferAi.Scenting`: lifts its nose to scent the air.
struct Scenting;

impl Behavior for Scenting {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &SCENTING_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        40
    }

    fn max_duration(&self) -> i32 {
        80
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        !SnifferEntity::is_tempted(tick.state)
    }

    fn can_still_use(&mut self, _tick: &mut BrainTick<'_>) -> bool {
        true
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::Scenting);
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::Idling);
        }
    }

    fn debug_name(&self) -> &'static str {
        "Scenting"
    }
}

const SNIFFING_CONDITIONS: [(MemoryModuleId, MemoryStatus); 3] = [
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
    (
        types::SNIFFER_SNIFFING_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (types::SNIFF_COOLDOWN.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `SnifferAi.Sniffing`: sniffs the ground, then heads for a spot worth digging.
struct Sniffing;

impl Behavior for Sniffing {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &SNIFFING_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        40
    }

    fn max_duration(&self) -> i32 {
        80
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        sniffer(tick).is_some_and(|sniffer| !sniffer.is_baby() && sniffer.can_sniff(tick.state))
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        sniffer(tick).is_some_and(|sniffer| sniffer.can_sniff(tick.state))
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::Sniffing);
        }
    }

    fn stop_timed_out(&mut self, tick: &mut BrainTick<'_>, timed_out: bool) {
        let Some(sniffer) = sniffer(tick) else {
            return;
        };
        sniffer.transition_to(SnifferState::Idling);
        if timed_out && let Some(pos) = sniffer.calculate_dig_position(tick.state) {
            tick.state.set(types::SNIFFER_SNIFFING_TARGET, pos);
            tick.state.set(
                types::WALK_TARGET,
                WalkTarget::from_block_pos(pos, SPEED_MULTIPLIER_WHEN_SNIFFING, 0),
            );
        }
    }

    fn debug_name(&self) -> &'static str {
        "Sniffing"
    }
}

const SEARCHING_CONDITIONS: [(MemoryModuleId, MemoryStatus); 3] = [
    (types::WALK_TARGET.id(), MemoryStatus::ValuePresent),
    (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
    (
        types::SNIFFER_SNIFFING_TARGET.id(),
        MemoryStatus::ValuePresent,
    ),
];

/// Vanilla `SnifferAi.Searching`: walks to the sniffed spot and starts digging there.
struct Searching;

impl Behavior for Searching {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &SEARCHING_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        600
    }

    fn max_duration(&self) -> i32 {
        600
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        sniffer(tick).is_some_and(|sniffer| sniffer.can_sniff(tick.state))
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let Some(sniffer) = sniffer(tick) else {
            return false;
        };
        if !sniffer.can_sniff(tick.state) {
            sniffer.transition_to(SnifferState::Idling);
            return false;
        }
        let walk_to = tick
            .state
            .get(types::WALK_TARGET)
            .map(|walk| walk.target().current_block_position());
        let sniffing_target = tick.state.get(types::SNIFFER_SNIFFING_TARGET).copied();
        walk_to.is_some() && walk_to == sniffing_target
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::Searching);
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick)
            && sniffer.can_dig(tick.state)
            && sniffer.can_sniff(tick.state)
        {
            tick.state.set(types::SNIFFER_DIGGING, true);
        }
        tick.state.erase(types::WALK_TARGET.id());
        tick.state.erase(types::SNIFFER_SNIFFING_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "Searching"
    }
}

const DIGGING_CONDITIONS: [(MemoryModuleId, MemoryStatus); 4] = [
    (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::SNIFFER_DIGGING.id(), MemoryStatus::ValuePresent),
    (types::SNIFF_COOLDOWN.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `SnifferAi.Digging`: a full dig starts the sniffing cooldown, an interrupted one
/// gives up on the spot.
struct Digging;

impl Behavior for Digging {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &DIGGING_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        160
    }

    fn max_duration(&self) -> i32 {
        180
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        sniffer(tick).is_some_and(|sniffer| sniffer.can_sniff(tick.state))
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::SNIFFER_DIGGING.id())
            && sniffer(tick).is_some_and(|sniffer| {
                sniffer.can_dig(tick.state) && !sniffer.mob_entity.is_in_love()
            })
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::Digging);
        }
    }

    fn stop_timed_out(&mut self, tick: &mut BrainTick<'_>, timed_out: bool) {
        if timed_out {
            tick.state
                .set_with_expiry(types::SNIFF_COOLDOWN, (), SNIFFING_COOLDOWN_TICKS);
        } else if let Some(sniffer) = sniffer(tick) {
            sniffer.reset_sniffing(tick.state);
        }
    }

    fn debug_name(&self) -> &'static str {
        "Digging"
    }
}

const FINISHED_DIGGING_CONDITIONS: [(MemoryModuleId, MemoryStatus); 4] = [
    (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::SNIFFER_DIGGING.id(), MemoryStatus::ValuePresent),
    (types::SNIFF_COOLDOWN.id(), MemoryStatus::ValuePresent),
];

/// Vanilla `SnifferAi.FinishedDigging`: rises out of the hole and gets happy.
struct FinishedDigging;

impl Behavior for FinishedDigging {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &FINISHED_DIGGING_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        40
    }

    fn max_duration(&self) -> i32 {
        40
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::SNIFFER_DIGGING.id())
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::Rising);
        }
    }

    fn stop_timed_out(&mut self, tick: &mut BrainTick<'_>, timed_out: bool) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::Idling);
            sniffer.on_digging_complete(tick.state, timed_out);
        }
        tick.state.erase(types::SNIFFER_DIGGING.id());
        tick.state.set(types::SNIFFER_HAPPY, true);
    }

    fn debug_name(&self) -> &'static str {
        "FinishedDigging"
    }
}

const FEELING_HAPPY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 1] =
    [(types::SNIFFER_HAPPY.id(), MemoryStatus::ValuePresent)];

/// Vanilla `SnifferAi.FeelingHappy`.
struct FeelingHappy;

impl Behavior for FeelingHappy {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &FEELING_HAPPY_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        40
    }

    fn max_duration(&self) -> i32 {
        100
    }

    fn can_still_use(&mut self, _tick: &mut BrainTick<'_>) -> bool {
        true
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::FeelingHappy);
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(sniffer) = sniffer(tick) {
            sniffer.transition_to(SnifferState::Idling);
        }
        tick.state.erase(types::SNIFFER_HAPPY.id());
    }

    fn debug_name(&self) -> &'static str {
        "FeelingHappy"
    }
}
