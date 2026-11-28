//! Vanilla `VaultBlockEntity` with its `VaultConfig`, `VaultServerData`, `VaultSharedData` and the
//! `VaultState` machine (`VaultBlockEntity.Server`).

use super::BlockEntity;
use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::plugin::api::events::block::vault_display_item::VaultDisplayItemEvent;
use crate::world::World;
use crate::world::loot::{LootContextParameters, generate_loot_from_handle};
use pumpkin_data::Block;
use pumpkin_data::block_properties::{VaultLikeProperties, VaultState};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::world::WorldEvent;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Vanilla `VaultServerData.MAX_REWARD_PLAYERS`.
const MAX_REWARD_PLAYERS: usize = 128;
/// Vanilla `VaultBlockEntity.Server.UNLOCKING_DELAY_TICKS`.
const UNLOCKING_DELAY_TICKS: i64 = 14;
/// Vanilla `VaultBlockEntity.Server.DISPLAY_CYCLE_TICK_RATE`.
const DISPLAY_CYCLE_TICK_RATE: i64 = 20;
/// Vanilla `VaultBlockEntity.Server.INSERT_FAIL_SOUND_BUFFER_TICKS`.
const INSERT_FAIL_SOUND_BUFFER_TICKS: i64 = 15;
/// Vanilla `VaultState.UPDATE_CONNECTED_PLAYERS_TICK_RATE`.
const UPDATE_CONNECTED_PLAYERS_TICK_RATE: i64 = 20;
/// Vanilla `VaultState.DELAY_BETWEEN_EJECTIONS_TICKS` and `DELAY_AFTER_LAST_EJECTION_TICKS`.
const DELAY_BETWEEN_EJECTIONS_TICKS: i64 = 20;
/// Vanilla `VaultState.DELAY_BEFORE_FIRST_EJECTION_TICKS`.
const DELAY_BEFORE_FIRST_EJECTION_TICKS: i64 = 20;

const DEFAULT_LOOT_TABLE: &str = "minecraft:chests/trial_chambers/reward";
const DEFAULT_ACTIVATION_RANGE: f64 = 4.0;
const DEFAULT_DEACTIVATION_RANGE: f64 = 4.5;

/// Vanilla `VaultConfig`. Placed vaults use [`Self::default`]; trial chamber templates carry
/// their own (ominous vaults take ominous keys and the ominous reward table).
#[derive(Clone)]
pub struct VaultConfig {
    pub loot_table: String,
    pub activation_range: f64,
    pub deactivation_range: f64,
    pub key_item: ItemStack,
    pub override_loot_table_to_display: Option<String>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            loot_table: DEFAULT_LOOT_TABLE.to_string(),
            activation_range: DEFAULT_ACTIVATION_RANGE,
            deactivation_range: DEFAULT_DEACTIVATION_RANGE,
            key_item: ItemStack::new(1, &Item::TRIAL_KEY),
            override_loot_table_to_display: None,
        }
    }
}

impl VaultConfig {
    /// Vanilla `VaultConfig.CODEC`: every field is lenient and falls back to the default.
    #[must_use]
    pub fn from_nbt(nbt: &NbtCompound) -> Self {
        let default = Self::default();
        let activation_range = nbt
            .get_double("activation_range")
            .unwrap_or(default.activation_range);
        let deactivation_range = nbt
            .get_double("deactivation_range")
            .unwrap_or(default.deactivation_range);
        // Vanilla `VaultConfig.validate`: the deactivation range must not be smaller.
        let (activation_range, deactivation_range) = if activation_range > deactivation_range {
            (default.activation_range, default.deactivation_range)
        } else {
            (activation_range, deactivation_range)
        };
        Self {
            loot_table: nbt
                .get_string("loot_table")
                .map_or(default.loot_table, str::to_string),
            activation_range,
            deactivation_range,
            key_item: nbt
                .get_compound("key_item")
                .and_then(ItemStack::read_item_stack)
                .unwrap_or(default.key_item),
            override_loot_table_to_display: nbt
                .get_string("override_loot_table_to_display")
                .map(str::to_string),
        }
    }

    #[must_use]
    pub fn to_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        if self.loot_table != DEFAULT_LOOT_TABLE {
            nbt.put_string("loot_table", self.loot_table.clone());
        }
        if self.activation_range != DEFAULT_ACTIVATION_RANGE {
            nbt.put_double("activation_range", self.activation_range);
        }
        if self.deactivation_range != DEFAULT_DEACTIVATION_RANGE {
            nbt.put_double("deactivation_range", self.deactivation_range);
        }
        if let Some(key) = self.key_item.to_nbt() {
            nbt.put_compound("key_item", key);
        }
        if let Some(table) = &self.override_loot_table_to_display {
            nbt.put_string("override_loot_table_to_display", table.clone());
        }
        nbt
    }
}

/// Vanilla `VaultServerData`.
#[derive(Default)]
pub struct VaultServerData {
    /// Insertion order is kept: past 128 players the oldest one is forgotten.
    pub rewarded_players: Vec<Uuid>,
    pub state_updating_resumes_at: i64,
    pub items_to_eject: Vec<ItemStack>,
    pub last_insert_fail_timestamp: i64,
    pub total_ejections_needed: i32,
    is_dirty: bool,
}

impl VaultServerData {
    fn from_nbt(nbt: &NbtCompound) -> Self {
        let rewarded_players = nbt
            .get_list("rewarded_players")
            .map(|list| list.iter().filter_map(NbtTag::extract_uuid).collect())
            .unwrap_or_default();
        let items_to_eject = nbt
            .get_list("items_to_eject")
            .map(|list| {
                list.iter()
                    .filter_map(NbtTag::extract_compound)
                    .filter_map(ItemStack::read_item_stack)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            rewarded_players,
            state_updating_resumes_at: nbt.get_long("state_updating_resumes_at").unwrap_or(0),
            items_to_eject,
            last_insert_fail_timestamp: 0,
            total_ejections_needed: nbt.get_int("total_ejections_needed").unwrap_or(0),
            is_dirty: false,
        }
    }

    fn to_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_list(
            "rewarded_players",
            self.rewarded_players
                .iter()
                .map(|id| NbtTag::uuid(*id))
                .collect(),
        );
        nbt.put_long("state_updating_resumes_at", self.state_updating_resumes_at);
        nbt.put_list(
            "items_to_eject",
            self.items_to_eject
                .iter()
                .filter_map(ItemStack::to_nbt)
                .map(NbtTag::Compound)
                .collect(),
        );
        nbt.put_int("total_ejections_needed", self.total_ejections_needed);
        nbt
    }

    #[must_use]
    pub fn has_rewarded_player(&self, player: &Uuid) -> bool {
        self.rewarded_players.contains(player)
    }

    /// Vanilla `addToRewardedPlayers`: past the limit the oldest entry is dropped.
    pub fn add_to_rewarded_players(&mut self, player: Uuid) {
        if !self.rewarded_players.contains(&player) {
            self.rewarded_players.push(player);
        }
        if self.rewarded_players.len() > MAX_REWARD_PLAYERS {
            self.rewarded_players.remove(0);
        }
        self.is_dirty = true;
    }

    const fn pause_state_updating_until(&mut self, time: i64) {
        self.state_updating_resumes_at = time;
        self.is_dirty = true;
    }

    const fn mark_ejection_finished(&mut self) {
        self.total_ejections_needed = 0;
        self.is_dirty = true;
    }

    fn set_items_to_eject(&mut self, items: Vec<ItemStack>) {
        self.items_to_eject = items;
        self.total_ejections_needed = i32::try_from(self.items_to_eject.len()).unwrap_or(i32::MAX);
        self.is_dirty = true;
    }

    /// Vanilla ejects from the end of the list.
    fn next_item_to_eject(&self) -> ItemStack {
        self.items_to_eject
            .last()
            .cloned()
            .unwrap_or_else(|| ItemStack::EMPTY.clone())
    }

    fn pop_next_item_to_eject(&mut self) -> ItemStack {
        if self.items_to_eject.is_empty() {
            return ItemStack::EMPTY.clone();
        }
        self.is_dirty = true;
        self.items_to_eject
            .pop()
            .unwrap_or_else(|| ItemStack::EMPTY.clone())
    }

    /// Vanilla `ejectionProgress`: `1 - inverseLerp(left, 1, total)`.
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    fn ejection_progress(&self) -> f32 {
        if self.total_ejections_needed == 1 {
            return 1.0;
        }
        let left = self.items_to_eject.len() as f32;
        let total = self.total_ejections_needed as f32;
        1.0 - (left - 1.0) / (total - 1.0)
    }
}

/// Vanilla `VaultSharedData`: the part the client sees (display item, connected players).
pub struct VaultSharedData {
    pub display_item: ItemStack,
    pub connected_players: Vec<Uuid>,
    pub connected_particles_range: f64,
    is_dirty: bool,
}

impl Default for VaultSharedData {
    fn default() -> Self {
        Self {
            display_item: ItemStack::EMPTY.clone(),
            connected_players: Vec::new(),
            connected_particles_range: DEFAULT_DEACTIVATION_RANGE,
            is_dirty: false,
        }
    }
}

impl VaultSharedData {
    fn from_nbt(nbt: &NbtCompound) -> Self {
        Self {
            display_item: nbt
                .get_compound("display_item")
                .and_then(ItemStack::read_item_stack)
                .unwrap_or_else(|| ItemStack::EMPTY.clone()),
            connected_players: nbt
                .get_list("connected_players")
                .map(|list| list.iter().filter_map(NbtTag::extract_uuid).collect())
                .unwrap_or_default(),
            connected_particles_range: nbt
                .get_double("connected_particles_range")
                .unwrap_or(DEFAULT_DEACTIVATION_RANGE),
            is_dirty: false,
        }
    }

    fn to_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        if let Some(item) = self.display_item.to_nbt() {
            nbt.put_compound("display_item", item);
        }
        nbt.put_list(
            "connected_players",
            self.connected_players
                .iter()
                .map(|id| NbtTag::uuid(*id))
                .collect(),
        );
        nbt.put_double("connected_particles_range", self.connected_particles_range);
        nbt
    }

    #[must_use]
    pub const fn has_display_item(&self) -> bool {
        !self.display_item.is_empty()
    }

    /// Vanilla `setDisplayItem`: only an actual change marks the data dirty.
    pub fn set_display_item(&mut self, item: ItemStack) {
        let same = (self.display_item.is_empty() && item.is_empty())
            || (self.display_item.item_count == item.item_count
                && self.display_item.are_items_and_components_equal(&item));
        if same {
            return;
        }
        self.display_item = item;
        self.is_dirty = true;
    }

    fn set_connected_players(&mut self, players: Vec<Uuid>) {
        // Vanilla compares as sets.
        let same = players.len() == self.connected_players.len()
            && players.iter().all(|id| self.connected_players.contains(id));
        if !same {
            self.connected_players = players;
            self.is_dirty = true;
        }
    }
}

/// Config, server and shared data under one lock.
#[derive(Default)]
pub struct VaultData {
    pub config: VaultConfig,
    pub server: VaultServerData,
    pub shared: VaultSharedData,
}

pub struct VaultBlockEntity {
    pub position: BlockPos,
    pub data: Mutex<VaultData>,
    dirty: AtomicBool,
}

impl BlockEntity for VaultBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let data = VaultData {
            config: nbt
                .get_compound("config")
                .map(VaultConfig::from_nbt)
                .unwrap_or_default(),
            server: nbt
                .get_compound("server_data")
                .map(VaultServerData::from_nbt)
                .unwrap_or_default(),
            shared: nbt
                .get_compound("shared_data")
                .map(VaultSharedData::from_nbt)
                .unwrap_or_default(),
        };
        Self {
            position,
            data: Mutex::new(data),
            dirty: AtomicBool::new(false),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        let data = self.lock();
        nbt.put_compound("config", data.config.to_nbt());
        nbt.put_compound("shared_data", data.shared.to_nbt());
        nbt.put_compound("server_data", data.server.to_nbt());
    }

    /// Vanilla `getUpdateTag`: only the shared data.
    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        nbt.put_compound("shared_data", self.lock().shared.to_nbt());
        Some(nbt)
    }

    fn tick(&self, world: &Arc<World>) {
        self.server_tick(world);
    }

    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// What a state change has to do once the data lock is released.
#[derive(Clone, Copy)]
struct Transition {
    from: VaultState,
    to: VaultState,
}

impl VaultBlockEntity {
    pub const ID: &'static str = "minecraft:vault";

    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            data: Mutex::new(VaultData::default()),
            dirty: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, VaultData> {
        self.data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[must_use]
    pub fn has_rewarded(&self, player_id: &Uuid) -> bool {
        self.lock().server.has_rewarded_player(player_id)
    }

    fn state_props(world: &World, pos: &BlockPos) -> Option<VaultLikeProperties> {
        let (block, state) = world.get_block_and_state_id(pos);
        (block == &Block::VAULT).then(|| VaultLikeProperties::from_state_id(state))
    }

    /// Vanilla `VaultBlockEntity.Server.tick`.
    fn server_tick(&self, world: &Arc<World>) {
        let Some(props) = Self::state_props(world, &self.position) else {
            return;
        };
        let state = props.vault_state;
        let game_time = world.get_world_age();
        if game_time % DISPLAY_CYCLE_TICK_RATE == 0 && state == VaultState::Active {
            self.cycle_display_item_from_loot_table(world, state);
        }

        let transition = {
            let mut data = self.lock();
            if game_time >= data.server.state_updating_resumes_at {
                let next = self.tick_and_get_next(world, state, game_time, &mut data);
                (next != state).then_some(Transition {
                    from: state,
                    to: next,
                })
            } else {
                None
            }
        };
        if let Some(transition) = transition {
            self.set_vault_state(world, props, transition);
        }
        self.flush_dirty(world);
    }

    /// Vanilla: `setChanged`, and a block update for the clients when the shared data changed.
    fn flush_dirty(&self, world: &Arc<World>) {
        let (server_dirty, shared_dirty) = {
            let mut data = self.lock();
            let dirty = (data.server.is_dirty, data.shared.is_dirty);
            data.server.is_dirty = false;
            data.shared.is_dirty = false;
            dirty
        };
        if server_dirty || shared_dirty {
            self.dirty.store(true, Ordering::Relaxed);
            if shared_dirty && let Some(entity) = world.get_block_entity(&self.position) {
                world.update_block_entity(&entity);
            }
        }
    }

    /// Vanilla `VaultState.tickAndGetNext`.
    fn tick_and_get_next(
        &self,
        world: &Arc<World>,
        state: VaultState,
        game_time: i64,
        data: &mut VaultData,
    ) -> VaultState {
        match state {
            VaultState::Inactive => {
                let range = data.config.activation_range;
                self.update_state_for_connected_players(world, game_time, data, range)
            }
            VaultState::Active => {
                let range = data.config.deactivation_range;
                self.update_state_for_connected_players(world, game_time, data, range)
            }
            VaultState::Unlocking => {
                data.server
                    .pause_state_updating_until(game_time + DELAY_BEFORE_FIRST_EJECTION_TICKS);
                VaultState::Ejecting
            }
            VaultState::Ejecting => {
                if data.server.items_to_eject.is_empty() {
                    data.server.mark_ejection_finished();
                    let range = data.config.deactivation_range;
                    self.update_state_for_connected_players(world, game_time, data, range)
                } else {
                    let progress = data.server.ejection_progress();
                    let item = data.server.pop_next_item_to_eject();
                    self.eject_result_item(world, item, progress);
                    let next = data.server.next_item_to_eject();
                    data.shared.set_display_item(next);
                    // Vanilla waits 20 ticks after every ejection, the last one included.
                    data.server
                        .pause_state_updating_until(game_time + DELAY_BETWEEN_EJECTIONS_TICKS);
                    VaultState::Ejecting
                }
            }
        }
    }

    /// Vanilla `VaultState.updateStateForConnectedPlayers`.
    fn update_state_for_connected_players(
        &self,
        world: &World,
        game_time: i64,
        data: &mut VaultData,
        range: f64,
    ) -> VaultState {
        self.update_connected_players_within_range(world, data, range);
        data.server
            .pause_state_updating_until(game_time + UPDATE_CONNECTED_PLAYERS_TICK_RATE);
        if data.shared.connected_players.is_empty() {
            VaultState::Inactive
        } else {
            VaultState::Active
        }
    }

    /// Vanilla `VaultSharedData.updateConnectedPlayersWithinRange` with
    /// `PlayerDetector.INCLUDING_CREATIVE_PLAYERS`: non-spectators whose block position is closer
    /// than `range`, minus the players already rewarded.
    fn update_connected_players_within_range(
        &self,
        world: &World,
        data: &mut VaultData,
        range: f64,
    ) {
        let center = self.position.0;
        let players = world
            .players
            .load()
            .iter()
            .filter(|player| {
                if player.is_spectator() {
                    return false;
                }
                // Vanilla `Vec3i.closerThan`: integer block coordinates, strictly closer.
                let pos = player.get_entity().block_pos.load().0;
                let dx = f64::from(pos.x - center.x);
                let dy = f64::from(pos.y - center.y);
                let dz = f64::from(pos.z - center.z);
                dx.mul_add(dx, dy.mul_add(dy, dz * dz)) < range * range
            })
            .map(|player| player.gameprofile.id)
            .filter(|id| !data.server.has_rewarded_player(id))
            .collect();
        data.shared.set_connected_players(players);
    }

    /// Vanilla `VaultBlockEntity.Server.setVaultState` and `VaultState.onTransition`.
    fn set_vault_state(
        &self,
        world: &Arc<World>,
        mut props: VaultLikeProperties,
        transition: Transition,
    ) {
        // Paper `VaultChangeStateEvent`
        if world
            .plugin_event(|| {
                crate::plugin::api::events::block::vault_change_state::VaultChangeStateEvent {
                    block: self.position,
                    current_state: format!("{:?}", transition.from).to_uppercase(),
                    new_state: format!("{:?}", transition.to).to_uppercase(),
                    cancelled: false,
                }
            })
            .is_some_and(|event| event.cancelled)
        {
            return;
        }
        props.vault_state = transition.to;
        world.set_block_state(
            &self.position,
            props.to_state_id(&Block::VAULT),
            BlockFlags::NOTIFY_ALL,
        );
        self.on_exit(world, transition.from);
        self.on_enter(world, transition.to, props.ominous);
    }

    fn on_exit(&self, world: &World, state: VaultState) {
        if state == VaultState::Ejecting {
            self.play_sound(world, Sound::BlockVaultCloseShutter, 1.0, 1.0);
        }
    }

    fn on_enter(&self, world: &Arc<World>, state: VaultState, ominous: bool) {
        match state {
            VaultState::Inactive => {
                self.lock()
                    .shared
                    .set_display_item(ItemStack::EMPTY.clone());
                world.sync_world_event(
                    WorldEvent::AnimationVaultDeactivate,
                    self.position,
                    i32::from(ominous),
                );
            }
            VaultState::Active => {
                if !self.lock().shared.has_display_item() {
                    self.cycle_display_item_from_loot_table(world, state);
                }
                world.sync_world_event(
                    WorldEvent::AnimationVaultActivate,
                    self.position,
                    i32::from(ominous),
                );
            }
            VaultState::Unlocking => {
                self.play_sound(world, Sound::BlockVaultInsertItem, 1.0, 1.0);
            }
            VaultState::Ejecting => {
                self.play_sound(world, Sound::BlockVaultOpenShutter, 1.0, 1.0);
            }
        }
    }

    fn play_sound(&self, world: &World, sound: Sound, volume: f32, pitch: f32) {
        world.play_sound_fine(
            sound,
            SoundCategory::Blocks,
            &self.position.to_centered_f64(),
            volume,
            pitch,
        );
    }

    /// Vanilla `canEjectReward`: a key is configured and the vault isn't inactive.
    fn can_eject_reward(config: &VaultConfig, state: VaultState) -> bool {
        !config.key_item.is_empty() && state != VaultState::Inactive
    }

    /// Vanilla `cycleDisplayItemFromLootTable`, with Paper's `VaultDisplayItemEvent`.
    fn cycle_display_item_from_loot_table(&self, world: &Arc<World>, state: VaultState) {
        let (can_eject, table) = {
            let data = self.lock();
            (
                Self::can_eject_reward(&data.config, state),
                data.config
                    .override_loot_table_to_display
                    .clone()
                    .unwrap_or_else(|| data.config.loot_table.clone()),
            )
        };
        if !can_eject {
            self.lock()
                .shared
                .set_display_item(ItemStack::EMPTY.clone());
            return;
        }
        let mut item = self.random_display_item_from_loot_table(world, &table);
        // The data lock is not held: a handler may read this vault.
        if let Some(event) = world
            .plugin_event(|| VaultDisplayItemEvent::new(self.position, world.clone(), item.clone()))
        {
            if event.cancelled {
                return;
            }
            item = event.item;
        }
        self.lock().shared.set_display_item(item);
    }

    /// Vanilla `getRandomDisplayItemFromLootTable`: one random stack of a roll.
    fn random_display_item_from_loot_table(&self, world: &World, table: &str) -> ItemStack {
        let Some(handle) = world.get_loot_table(table) else {
            return ItemStack::EMPTY.clone();
        };
        let items = generate_loot_from_handle(
            &handle,
            rand::random(),
            &LootContextParameters {
                world: Some(world),
                position: Some(self.position.to_centered_f64()),
                ..Default::default()
            },
        );
        if items.is_empty() {
            return ItemStack::EMPTY.clone();
        }
        items[rand::random_range(0..items.len())].clone()
    }

    /// Vanilla `VaultState.ejectResultItem`.
    fn eject_result_item(&self, world: &Arc<World>, item: ItemStack, progress: f32) {
        // `Vec3.atBottomCenterOf(pos).relative(UP, 1.2)`.
        let position = Vector3::new(
            f64::from(self.position.0.x) + 0.5,
            f64::from(self.position.0.y) + 1.2,
            f64::from(self.position.0.z) + 0.5,
        );
        crate::block::blocks::redstone::dispenser::spawn_dispensed_item(
            world,
            item,
            2,
            pumpkin_data::BlockDirection::Up,
            position,
        );
        world.sync_world_event(WorldEvent::AnimationVaultEjectItem, self.position, 0);
        self.play_sound(
            world,
            Sound::BlockVaultEjectItem,
            1.0,
            0.4f32.mul_add(progress, 0.8),
        );
    }

    /// Vanilla `VaultBlockEntity.Server.tryInsertKey`, from `VaultBlock.useItemOn`.
    pub fn try_insert_key(&self, world: &Arc<World>, player: &Arc<Player>, stack: &mut ItemStack) {
        let Some(props) = Self::state_props(world, &self.position) else {
            return;
        };
        let game_time = world.get_world_age();
        let config = self.lock().config.clone();
        if !Self::can_eject_reward(&config, props.vault_state) {
            return;
        }
        if !Self::is_valid_to_insert(&config, stack) {
            self.play_insert_fail_sound(world, game_time, Sound::BlockVaultInsertItemFail);
            return;
        }
        if self.has_rewarded(&player.gameprofile.id) {
            self.play_insert_fail_sound(world, game_time, Sound::BlockVaultRejectRewardedPlayer);
            return;
        }
        let items = self.resolve_items_to_eject(world, &config, player, stack);
        if items.is_empty() {
            return;
        }
        player.increment_stat(
            pumpkin_data::statistic::StatisticCategory::Used,
            i32::from(stack.item.id),
            1,
        );
        stack.decrement_unless_creative(player.gamemode.load(), config.key_item.item_count);
        self.unlock(world, props, game_time, items);
        let mut data = self.lock();
        data.server.add_to_rewarded_players(player.gameprofile.id);
        let range = data.config.deactivation_range;
        self.update_connected_players_within_range(world, &mut data, range);
    }

    /// Vanilla `isValidToInsert`: the configured key, at least as many as configured.
    fn is_valid_to_insert(config: &VaultConfig, stack: &ItemStack) -> bool {
        !stack.is_empty()
            && stack.are_items_and_components_equal(&config.key_item)
            && stack.item_count >= config.key_item.item_count
    }

    /// Vanilla `playInsertFailSound`: at most once every 15 ticks.
    fn play_insert_fail_sound(&self, world: &World, game_time: i64, sound: Sound) {
        let mut data = self.lock();
        if game_time >= data.server.last_insert_fail_timestamp + INSERT_FAIL_SOUND_BUFFER_TICKS {
            self.play_sound(world, sound, 1.0, 1.0);
            data.server.last_insert_fail_timestamp = game_time;
        }
    }

    /// Vanilla `resolveItemsToEject`, with Paper's `BlockDispenseLootEvent`.
    fn resolve_items_to_eject(
        &self,
        world: &Arc<World>,
        config: &VaultConfig,
        player: &Arc<Player>,
        key: &ItemStack,
    ) -> Vec<ItemStack> {
        let Some(handle) = world.get_loot_table(&config.loot_table) else {
            return Vec::new();
        };
        let items = generate_loot_from_handle(
            &handle,
            0,
            &LootContextParameters {
                world: Some(world),
                position: Some(self.position.to_centered_f64()),
                luck: player
                    .living_entity
                    .get_attribute_value(&pumpkin_data::attributes::Attributes::LUCK)
                    as f32,
                this_entity: Some(player.as_ref() as &dyn EntityBase),
                tool: Some(key.clone()),
                ..Default::default()
            },
        );
        let mut event =
            crate::plugin::api::events::block::block_dispense_loot::BlockDispenseLootEvent {
                block_pos: self.position,
                world: world.clone(),
                items,
                cancelled: false,
            };
        if world.fire_plugin_event(&mut event) {
            return Vec::new();
        }
        event.items
    }

    /// Vanilla `unlock`.
    fn unlock(
        &self,
        world: &Arc<World>,
        props: VaultLikeProperties,
        game_time: i64,
        items: Vec<ItemStack>,
    ) {
        let mut data = self.lock();
        data.server.set_items_to_eject(items);
        let next = data.server.next_item_to_eject();
        data.shared.set_display_item(next);
        data.server
            .pause_state_updating_until(game_time + UNLOCKING_DELAY_TICKS);
        drop(data);
        self.set_vault_state(
            world,
            props,
            Transition {
                from: props.vault_state,
                to: VaultState::Unlocking,
            },
        );
        self.flush_dirty(world);
    }
}

#[cfg(test)]
mod tests {
    use super::{VaultConfig, VaultServerData, VaultSharedData};
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    use uuid::Uuid;

    #[test]
    fn config_round_trips_and_defaults() {
        let config = VaultConfig {
            loot_table: "minecraft:chests/trial_chambers/reward_ominous".to_string(),
            key_item: ItemStack::new(1, &Item::OMINOUS_TRIAL_KEY),
            ..VaultConfig::default()
        };
        let read = VaultConfig::from_nbt(&config.to_nbt());
        assert_eq!(read.loot_table, config.loot_table);
        assert_eq!(read.key_item.item.id, Item::OMINOUS_TRIAL_KEY.id);
        let empty = VaultConfig::from_nbt(&pumpkin_nbt::compound::NbtCompound::new());
        assert_eq!(empty.key_item.item.id, Item::TRIAL_KEY.id);
        assert!((empty.deactivation_range - 4.5).abs() < f64::EPSILON);
    }

    #[test]
    fn rewarded_players_forget_the_oldest_past_128() {
        let mut data = VaultServerData::default();
        let first = Uuid::from_u128(1);
        data.add_to_rewarded_players(first);
        for i in 2..=129 {
            data.add_to_rewarded_players(Uuid::from_u128(i));
        }
        assert_eq!(data.rewarded_players.len(), 128);
        assert!(!data.has_rewarded_player(&first));
        assert!(data.has_rewarded_player(&Uuid::from_u128(129)));
    }

    #[test]
    fn ejection_pops_from_the_end_and_tracks_progress() {
        let mut data = VaultServerData::default();
        data.set_items_to_eject(vec![
            ItemStack::new(1, &Item::DIAMOND),
            ItemStack::new(2, &Item::EMERALD),
            ItemStack::new(3, &Item::ARROW),
        ]);
        assert!((data.ejection_progress() - 0.0).abs() < f32::EPSILON);
        assert_eq!(data.pop_next_item_to_eject().item.id, Item::ARROW.id);
        assert!((data.ejection_progress() - 0.5).abs() < f32::EPSILON);
        assert_eq!(data.next_item_to_eject().item.id, Item::EMERALD.id);
    }

    #[test]
    fn shared_data_round_trips() {
        let mut shared = VaultSharedData::default();
        shared.set_display_item(ItemStack::new(1, &Item::DIAMOND));
        shared.set_connected_players(vec![Uuid::from_u128(7)]);
        let read = VaultSharedData::from_nbt(&shared.to_nbt());
        assert_eq!(read.display_item.item.id, Item::DIAMOND.id);
        assert_eq!(read.connected_players, vec![Uuid::from_u128(7)]);
    }
}
