//! Paper events added in bulk (second batch): conversions between the core events and their
//! WIT records.

use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_wasm_host_common::state::PluginHostState;
use tokio::sync::Mutex;
use wasmtime::component::Resource;

use crate::{
    events::{
        ToFromWasmEvent, cleanup_event, consume_player, consume_world, entity::consume_entity,
        from_wasm_block_position, from_wasm_position, to_wasm_block_position, to_wasm_position,
    },
    pumpkin::plugin::event::{
        AsyncChatDecorateEventData, AsyncPlayerConnectionConfigureEventData,
        AsyncPlayerSendCommandsEventData, AsyncPlayerSendSuggestionsEventData,
        AsyncTabCompleteEventData, BlockBreakBlockEventData, BlockDestroyEventData,
        BlockLockCheckEventData, CompostItemEventData, EnderDragonFireballHitEventData,
        EnderDragonFlameEventData, EnderDragonShootFireballEventData, EndermanEscapeEventData,
        EntityAttemptSmashAttackEventData, EntityAttemptSpinAttackEventData,
        EntityCollideWithEntityEventData, EntityCompostItemEventData, EntityConstructEventData,
        EntityDamageItemEventData, EntityEffectTickEventData, EntityEquipmentChangedEventData,
        EntityFertilizeEggEventData, EntityIgniteEventData, EntityPortalReadyEventData,
        EntityPushedByEntityAttackEventData, EntityTeleportEndGatewayEventData, EntityZapEventData,
        Event, FishHookStateChangeEventData, ItemCraftedEventData,
        ItemTransportingEntityValidateTargetEventData, PhantomPreSpawnEventData,
        PlayerAdvancementCriterionGrantEventData, PlayerAttemptPickupItemEventData,
        PlayerBedFailEnterEventData, PlayerChangeBeaconEffectEventData, PlayerChunkLoadEventData,
        PlayerChunkUnloadEventData, PlayerClientLoadedWorldEventData,
        PlayerConnectionInitialConfigureEventData, PlayerConnectionValidateLoginEventData,
        PlayerHandshakeEventData, PlayerInsertLecternBookEventData,
        PlayerInventorySlotChangeEventData, PlayerLecternPageChangeEventData,
        PlayerLoomPatternSelectEventData, PlayerNaturallySpawnCreaturesEventData,
        PlayerServerFullCheckEventData, PlayerSetSpawnEventData, PlayerShearBlockEventData,
        PlayerSignCommandPreprocessEventData, PlayerStonecutterRecipeSelectEventData,
        PlayerTeleportEndGatewayEventData, PlayerToggleEntityAgeLockEventData,
        PlayerUseUnknownEntityEventData, PluginDisableEventData, PluginEnableEventData,
        PreCreatureSpawnEventData, PreSpawnerSpawnEventData, ProfileWhitelistVerifyEventData,
        RemoteServerCommandEventData, ShulkerDuplicateEventData, SlimeChangeDirectionEventData,
        SlimeSwimEventData, SlimeTargetLivingEntityEventData, SlimeWanderEventData,
        StructuresLocateEventData, SulfurCubeSwallowItemEventData, TabCompleteEventData,
        TameableDeathMessageEventData, TurtleGoHomeEventData, TurtleLayEggEventData,
        TurtleStartDiggingEventData, UncheckedSignChangeEventData, UnknownCommandEventData,
        WaterBottleSplashEventData, WhitelistStateUpdateEventData, WhitelistToggleEventData,
        WitchConsumePotionEventData, WitchReadyPotionEventData, WitchThrowPotionEventData,
        WorldBorderBoundsChangeEventData, WorldBorderCenterChangeEventData,
    },
};

/// Takes an item stack resource out of the table.
fn take_item(
    state: &mut PluginHostState,
    item: &Resource<crate::pumpkin::plugin::item_stack::ItemStack>,
) -> ItemStack {
    let mutex = state
        .resource_table
        .delete::<Arc<Mutex<ItemStack>>>(Resource::new_own(item.rep()))
        .expect("invalid item stack resource handle");
    mutex.try_lock().expect("lock item stack").clone()
}

/// Reads an item stack resource a handler returned, leaving it for the cleanup.
fn peek_item(
    state: &PluginHostState,
    item: &Resource<crate::pumpkin::plugin::item_stack::ItemStack>,
) -> ItemStack {
    state
        .resource_table
        .get::<Arc<Mutex<ItemStack>>>(&Resource::new_borrow(item.rep()))
        .expect("invalid item stack resource handle")
        .try_lock()
        .expect("lock item stack")
        .clone()
}

/// Reads an entity resource a handler returned, leaving it for the cleanup.
fn peek_entity(
    state: &PluginHostState,
    entity: &Resource<crate::pumpkin::plugin::entity::Entity>,
) -> Option<Arc<dyn pumpkin_core::entity::EntityBase>> {
    state
        .resource_table
        .get::<Arc<dyn pumpkin_core::entity::EntityBase>>(&Resource::new_borrow(entity.rep()))
        .ok()
        .cloned()
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_bed_fail_enter::PlayerBedFailEnterEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerBedFailEnterEvent(PlayerBedFailEnterEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            fail_reason: self.fail_reason.clone(),
            bed: to_wasm_block_position(self.bed),
            will_explode: self.will_explode,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerBedFailEnterEvent(data) = &event {
            self.will_explode = data.will_explode;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerBedFailEnterEvent(data) => Self {
                player: consume_player(state, &data.player),
                fail_reason: data.fail_reason,
                bed: from_wasm_block_position(data.bed),
                will_explode: data.will_explode,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_shear_block::PlayerShearBlockEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerShearBlockEvent(PlayerShearBlockEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            block: to_wasm_block_position(self.block),
            item: state
                .add(Arc::new(Mutex::new(self.item.clone())))
                .expect("add resource"),
            hand: self.hand.clone(),
            drops: self
                .drops
                .iter()
                .map(|i| {
                    state
                        .add(Arc::new(Mutex::new(i.clone())))
                        .expect("add resource")
                })
                .collect(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerShearBlockEvent(data) = &event {
            self.drops = data.drops.iter().map(|i| peek_item(state, i)).collect();
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerShearBlockEvent(data) => Self {
                player: consume_player(state, &data.player),
                block: from_wasm_block_position(data.block),
                item: take_item(state, &data.item),
                hand: data.hand,
                drops: data.drops.iter().map(|i| take_item(state, i)).collect(),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_construct::EntityConstructEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityConstructEvent(EntityConstructEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            blocks: self
                .blocks
                .iter()
                .map(|p| to_wasm_block_position(*p))
                .collect(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityConstructEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityConstructEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                blocks: data
                    .blocks
                    .iter()
                    .map(|p| from_wasm_block_position(*p))
                    .collect(),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_compost_item::EntityCompostItemEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityCompostItemEvent(EntityCompostItemEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            block: to_wasm_block_position(self.block),
            item: state
                .add(Arc::new(Mutex::new(self.item.clone())))
                .expect("add resource"),
            will_raise_level: self.will_raise_level,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityCompostItemEvent(data) = &event {
            self.will_raise_level = data.will_raise_level;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityCompostItemEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                block: from_wasm_block_position(data.block),
                item: take_item(state, &data.item),
                will_raise_level: data.will_raise_level,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::block::compost_item::CompostItemEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::CompostItemEvent(CompostItemEventData {
            block: to_wasm_block_position(self.block),
            item: state
                .add(Arc::new(Mutex::new(self.item.clone())))
                .expect("add resource"),
            will_raise_level: self.will_raise_level,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::CompostItemEvent(data) = &event {
            self.will_raise_level = data.will_raise_level;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::CompostItemEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                item: take_item(state, &data.item),
                will_raise_level: data.will_raise_level,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_teleport_end_gateway::PlayerTeleportEndGatewayEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerTeleportEndGatewayEvent(PlayerTeleportEndGatewayEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            from_location: to_wasm_position(self.from),
            to_location: to_wasm_position(self.to),
            gateway: to_wasm_block_position(self.gateway),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerTeleportEndGatewayEvent(data) = &event {
            self.to = from_wasm_position(data.to_location);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerTeleportEndGatewayEvent(data) => Self {
                player: consume_player(state, &data.player),
                from: from_wasm_position(data.from_location),
                to: from_wasm_position(data.to_location),
                gateway: from_wasm_block_position(data.gateway),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::entity_teleport_end_gateway::EntityTeleportEndGatewayEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityTeleportEndGatewayEvent(EntityTeleportEndGatewayEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            from_location: to_wasm_position(self.from),
            to_location: to_wasm_position(self.to),
            gateway: to_wasm_block_position(self.gateway),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityTeleportEndGatewayEvent(data) = &event {
            self.to = from_wasm_position(data.to_location);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityTeleportEndGatewayEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                from: from_wasm_position(data.from_location),
                to: from_wasm_position(data.to_location),
                gateway: from_wasm_block_position(data.gateway),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_lectern_page_change::PlayerLecternPageChangeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerLecternPageChangeEvent(PlayerLecternPageChangeEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            lectern: to_wasm_block_position(self.lectern),
            book: state.add(Arc::new(Mutex::new(self.book.clone()))).expect("add resource"),
            page_turn: self.page_turn.clone(),
            old_page: self.old_page,
            new_page: self.new_page,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerLecternPageChangeEvent(data) = &event {
            self.new_page = data.new_page;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerLecternPageChangeEvent(data) => Self {
                player: consume_player(state, &data.player),
                lectern: from_wasm_block_position(data.lectern),
                book: take_item(state, &data.book),
                page_turn: data.page_turn,
                old_page: data.old_page,
                new_page: data.new_page,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_insert_lectern_book::PlayerInsertLecternBookEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerInsertLecternBookEvent(PlayerInsertLecternBookEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            block: to_wasm_block_position(self.block),
            book: state.add(Arc::new(Mutex::new(self.book.clone()))).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerInsertLecternBookEvent(data) = &event {
            self.book = peek_item(state, &data.book);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerInsertLecternBookEvent(data) => Self {
                player: consume_player(state, &data.player),
                block: from_wasm_block_position(data.block),
                book: take_item(state, &data.book),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_sign_command_preprocess::PlayerSignCommandPreprocessEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerSignCommandPreprocessEvent(PlayerSignCommandPreprocessEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            message: self.message.clone(),
            sign: to_wasm_block_position(self.sign),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerSignCommandPreprocessEvent(data) = &event {
            self.message.clone_from(&data.message);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerSignCommandPreprocessEvent(data) => Self {
                player: consume_player(state, &data.player),
                message: data.message,
                sign: from_wasm_block_position(data.sign),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::pre_spawner_spawn::PreSpawnerSpawnEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PreSpawnerSpawnEvent(PreSpawnerSpawnEventData {
            location: to_wasm_position(self.location),
            entity_type: self.entity_type.clone(),
            spawner_location: to_wasm_block_position(self.spawner_location),
            should_abort_spawn: self.should_abort_spawn,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PreSpawnerSpawnEvent(data) = &event {
            self.should_abort_spawn = data.should_abort_spawn;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PreSpawnerSpawnEvent(data) => Self {
                location: from_wasm_position(data.location),
                entity_type: data.entity_type,
                spawner_location: from_wasm_block_position(data.spawner_location),
                should_abort_spawn: data.should_abort_spawn,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::block_break_block::BlockBreakBlockEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::BlockBreakBlockEvent(BlockBreakBlockEventData {
            block: to_wasm_block_position(self.block),
            source: to_wasm_block_position(self.source),
            drops: self
                .drops
                .iter()
                .map(|i| {
                    state
                        .add(Arc::new(Mutex::new(i.clone())))
                        .expect("add resource")
                })
                .collect(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::BlockBreakBlockEvent(data) = &event {
            self.drops = data.drops.iter().map(|i| peek_item(state, i)).collect();
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::BlockBreakBlockEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                source: from_wasm_block_position(data.source),
                drops: data.drops.iter().map(|i| take_item(state, i)).collect(),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::async_player_send_commands::AsyncPlayerSendCommandsEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::AsyncPlayerSendCommandsEvent(AsyncPlayerSendCommandsEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            commands: self.commands.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::AsyncPlayerSendCommandsEvent(data) = &event {
            self.commands.clone_from(&data.commands);
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::AsyncPlayerSendCommandsEvent(data) => Self {
                player: consume_player(state, &data.player),
                commands: data.commands,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::world::structures_locate::StructuresLocateEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::StructuresLocateEvent(StructuresLocateEventData {
            event_world: state.add(self.world.clone()).expect("add resource"),
            origin: to_wasm_block_position(self.origin),
            structures: self.structures.clone(),
            radius: self.radius,
            find_unexplored: self.find_unexplored,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::StructuresLocateEvent(data) = &event {
            self.structures.clone_from(&data.structures);
            self.radius = data.radius;
            self.find_unexplored = data.find_unexplored;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::StructuresLocateEvent(data) => Self {
                world: consume_world(state, &data.event_world),
                origin: from_wasm_block_position(data.origin),
                structures: data.structures,
                radius: data.radius,
                find_unexplored: data.find_unexplored,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::server::whitelist_toggle::WhitelistToggleEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::WhitelistToggleEvent(WhitelistToggleEventData {
            enabled: self.enabled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::WhitelistToggleEvent(data) => Self {
                enabled: data.enabled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::server::whitelist_state_update::WhitelistStateUpdateEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::WhitelistStateUpdateEvent(WhitelistStateUpdateEventData {
            name: self.name.clone(),
            uuid: self.uuid.clone(),
            status: self.status.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::WhitelistStateUpdateEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::WhitelistStateUpdateEvent(data) => Self {
                name: data.name,
                uuid: data.uuid,
                status: data.status,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::world::world_border_bounds_change::WorldBorderBoundsChangeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::WorldBorderBoundsChangeEvent(WorldBorderBoundsChangeEventData {
            event_world: state.add(self.world.clone()).expect("add resource"),
            change_type: self.change_type.clone(),
            old_size: self.old_size,
            new_size: self.new_size,
            duration: self.duration,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::WorldBorderBoundsChangeEvent(data) = &event {
            self.new_size = data.new_size;
            self.duration = data.duration;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::WorldBorderBoundsChangeEvent(data) => Self {
                world: consume_world(state, &data.event_world),
                change_type: data.change_type,
                old_size: data.old_size,
                new_size: data.new_size,
                duration: data.duration,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::world::world_border_center_change::WorldBorderCenterChangeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::WorldBorderCenterChangeEvent(WorldBorderCenterChangeEventData {
            event_world: state.add(self.world.clone()).expect("add resource"),
            old_center_x: self.old_center_x,
            old_center_z: self.old_center_z,
            new_center_x: self.new_center_x,
            new_center_z: self.new_center_z,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::WorldBorderCenterChangeEvent(data) = &event {
            self.new_center_x = data.new_center_x;
            self.new_center_z = data.new_center_z;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::WorldBorderCenterChangeEvent(data) => Self {
                world: consume_world(state, &data.event_world),
                old_center_x: data.old_center_x,
                old_center_z: data.old_center_z,
                new_center_x: data.new_center_x,
                new_center_z: data.new_center_z,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::server::unknown_command::UnknownCommandEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::UnknownCommandEvent(UnknownCommandEventData {
            player: self
                .player
                .as_ref()
                .map(|p| state.add(p.clone()).expect("add resource")),
            command_line: self.command_line.clone(),
            message: self.message.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::UnknownCommandEvent(data) = &event {
            self.message.clone_from(&data.message);
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::UnknownCommandEvent(data) => Self {
                player: data.player.as_ref().map(|p| consume_player(state, p)),
                command_line: data.command_line,
                message: data.message,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_toggle_entity_age_lock::PlayerToggleEntityAgeLockEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerToggleEntityAgeLockEvent(PlayerToggleEntityAgeLockEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            entity: state.add(self.entity.clone()).expect("add resource"),
            item: state.add(Arc::new(Mutex::new(self.item.clone()))).expect("add resource"),
            age_locked: self.age_locked,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerToggleEntityAgeLockEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerToggleEntityAgeLockEvent(data) => Self {
                player: consume_player(state, &data.player),
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                item: take_item(state, &data.item),
                age_locked: data.age_locked,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::item_transporting_entity_validate_target::ItemTransportingEntityValidateTargetEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ItemTransportingEntityValidateTargetEvent(ItemTransportingEntityValidateTargetEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            block: to_wasm_block_position(self.block),
            allowed: self.allowed,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::ItemTransportingEntityValidateTargetEvent(data) = &event {
            self.allowed = data.allowed;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ItemTransportingEntityValidateTargetEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                block: from_wasm_block_position(data.block),
                allowed: data.allowed,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_fertilize_egg::EntityFertilizeEggEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityFertilizeEggEvent(EntityFertilizeEggEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            father: state.add(self.father.clone()).expect("add resource"),
            breeder: self
                .breeder
                .as_ref()
                .map(|p| state.add(p.clone()).expect("add resource")),
            experience: self.experience,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityFertilizeEggEvent(data) = &event {
            self.experience = data.experience;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityFertilizeEggEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                father: consume_entity(state, &data.father).expect("entity resource"),
                breeder: data.breeder.as_ref().map(|p| consume_player(state, p)),
                experience: data.experience,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::ender_dragon_flame::EnderDragonFlameEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EnderDragonFlameEvent(EnderDragonFlameEventData {
            dragon: state.add(self.dragon.clone()).expect("add resource"),
            area_effect_cloud: state
                .add(self.area_effect_cloud.clone())
                .expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EnderDragonFlameEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EnderDragonFlameEvent(data) => Self {
                dragon: consume_entity(state, &data.dragon).expect("entity resource"),
                area_effect_cloud: consume_entity(state, &data.area_effect_cloud)
                    .expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::ender_dragon_shoot_fireball::EnderDragonShootFireballEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EnderDragonShootFireballEvent(EnderDragonShootFireballEventData {
            dragon: state.add(self.dragon.clone()).expect("add resource"),
            fireball: state.add(self.fireball.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EnderDragonShootFireballEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EnderDragonShootFireballEvent(data) => Self {
                dragon: consume_entity(state, &data.dragon).expect("entity resource"),
                fireball: consume_entity(state, &data.fireball).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::entity_pushed_by_entity_attack::EntityPushedByEntityAttackEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityPushedByEntityAttackEvent(EntityPushedByEntityAttackEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            pushed_by: state.add(self.pushed_by.clone()).expect("add resource"),
            cause: self.cause.clone(),
            knockback: to_wasm_position(self.knockback),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityPushedByEntityAttackEvent(data) = &event {
            self.knockback = from_wasm_position(data.knockback);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityPushedByEntityAttackEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                pushed_by: consume_entity(state, &data.pushed_by).expect("entity resource"),
                cause: data.cause,
                knockback: from_wasm_position(data.knockback),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_attempt_pickup_item::PlayerAttemptPickupItemEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerAttemptPickupItemEvent(PlayerAttemptPickupItemEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            item_entity_id: self.item_entity_id,
            item: state.add(Arc::new(Mutex::new(self.item.clone()))).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerAttemptPickupItemEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerAttemptPickupItemEvent(data) => Self {
                player: consume_player(state, &data.player),
                item_entity_id: data.item_entity_id,
                item: take_item(state, &data.item),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::entity_attempt_spin_attack::EntityAttemptSpinAttackEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityAttemptSpinAttackEvent(EntityAttemptSpinAttackEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            target: state.add(self.target.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityAttemptSpinAttackEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityAttemptSpinAttackEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                target: consume_entity(state, &data.target).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::entity_equipment_changed::EntityEquipmentChangedEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityEquipmentChangedEvent(EntityEquipmentChangedEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            slots: self.slots.clone(),
            new_items: self.new_items.iter().map(|i| state.add(Arc::new(Mutex::new(i.clone()))).expect("add resource")).collect(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityEquipmentChangedEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                slots: data.slots,
                new_items: data.new_items.iter().map(|i| take_item(state, i)).collect(),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::entity_collide_with_entity::EntityCollideWithEntityEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityCollideWithEntityEvent(EntityCollideWithEntityEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            collided: state.add(self.collided.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityCollideWithEntityEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityCollideWithEntityEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                collided: consume_entity(state, &data.collided).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::tameable_death_message::TameableDeathMessageEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::TameableDeathMessageEvent(TameableDeathMessageEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            death_message: self.death_message.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::TameableDeathMessageEvent(data) = &event {
            self.death_message.clone_from(&data.death_message);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::TameableDeathMessageEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                death_message: data.death_message,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_effect_tick::EntityEffectTickEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityEffectTickEvent(EntityEffectTickEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            effect_type: self.effect_type.clone(),
            amplifier: self.amplifier,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityEffectTickEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityEffectTickEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                effect_type: data.effect_type,
                amplifier: data.amplifier,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_damage_item::EntityDamageItemEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityDamageItemEvent(EntityDamageItemEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            item: state
                .add(Arc::new(Mutex::new(self.item.clone())))
                .expect("add resource"),
            damage: self.damage,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityDamageItemEvent(data) = &event {
            self.damage = data.damage;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityDamageItemEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                item: take_item(state, &data.item),
                damage: data.damage,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::slime_swim::SlimeSwimEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::SlimeSwimEvent(SlimeSwimEventData {
            slime: state.add(self.slime.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::SlimeSwimEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::SlimeSwimEvent(data) => Self {
                slime: consume_entity(state, &data.slime).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::slime_change_direction::SlimeChangeDirectionEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::SlimeChangeDirectionEvent(SlimeChangeDirectionEventData {
            slime: state.add(self.slime.clone()).expect("add resource"),
            new_yaw: self.new_yaw,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::SlimeChangeDirectionEvent(data) = &event {
            self.new_yaw = data.new_yaw;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::SlimeChangeDirectionEvent(data) => Self {
                slime: consume_entity(state, &data.slime).expect("entity resource"),
                new_yaw: data.new_yaw,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::slime_wander::SlimeWanderEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::SlimeWanderEvent(SlimeWanderEventData {
            slime: state.add(self.slime.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::SlimeWanderEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::SlimeWanderEvent(data) => Self {
                slime: consume_entity(state, &data.slime).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::enderman_escape::EndermanEscapeEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EndermanEscapeEvent(EndermanEscapeEventData {
            enderman: state.add(self.enderman.clone()).expect("add resource"),
            reason: self.reason.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EndermanEscapeEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EndermanEscapeEvent(data) => Self {
                enderman: consume_entity(state, &data.enderman).expect("entity resource"),
                reason: data.reason,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::shulker_duplicate::ShulkerDuplicateEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ShulkerDuplicateEvent(ShulkerDuplicateEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            parent: state.add(self.parent.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::ShulkerDuplicateEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ShulkerDuplicateEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                parent: consume_entity(state, &data.parent).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::slime_target_living_entity::SlimeTargetLivingEntityEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::SlimeTargetLivingEntityEvent(SlimeTargetLivingEntityEventData {
            slime: state.add(self.slime.clone()).expect("add resource"),
            target: state.add(self.target.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::SlimeTargetLivingEntityEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::SlimeTargetLivingEntityEvent(data) => Self {
                slime: consume_entity(state, &data.slime).expect("entity resource"),
                target: consume_entity(state, &data.target).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_ignite::EntityIgniteEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityIgniteEvent(EntityIgniteEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            fuse_time: self.fuse_time,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityIgniteEvent(data) = &event {
            self.fuse_time = data.fuse_time;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityIgniteEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                fuse_time: data.fuse_time,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::sulfur_cube_swallow_item::SulfurCubeSwallowItemEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::SulfurCubeSwallowItemEvent(SulfurCubeSwallowItemEventData {
            sulfur_cube: state.add(self.sulfur_cube.clone()).expect("add resource"),
            player: state.add(self.player.clone()).expect("add resource"),
            old_item: state.add(Arc::new(Mutex::new(self.old_item.clone()))).expect("add resource"),
            new_item: state.add(Arc::new(Mutex::new(self.new_item.clone()))).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::SulfurCubeSwallowItemEvent(data) = &event {
            self.new_item = peek_item(state, &data.new_item);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::SulfurCubeSwallowItemEvent(data) => Self {
                sulfur_cube: consume_entity(state, &data.sulfur_cube).expect("entity resource"),
                player: consume_player(state, &data.player),
                old_item: take_item(state, &data.old_item),
                new_item: take_item(state, &data.new_item),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::witch_throw_potion::WitchThrowPotionEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::WitchThrowPotionEvent(WitchThrowPotionEventData {
            witch: state.add(self.witch.clone()).expect("add resource"),
            target: state.add(self.target.clone()).expect("add resource"),
            potion: self.potion.as_ref().map(|i| {
                state
                    .add(Arc::new(Mutex::new(i.clone())))
                    .expect("add resource")
            }),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::WitchThrowPotionEvent(data) = &event {
            self.potion = data.potion.as_ref().map(|i| peek_item(state, i));
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::WitchThrowPotionEvent(data) => Self {
                witch: consume_entity(state, &data.witch).expect("entity resource"),
                target: consume_entity(state, &data.target).expect("entity resource"),
                potion: data.potion.as_ref().map(|i| take_item(state, i)),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::witch_consume_potion::WitchConsumePotionEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::WitchConsumePotionEvent(WitchConsumePotionEventData {
            witch: state.add(self.witch.clone()).expect("add resource"),
            potion: self.potion.as_ref().map(|i| {
                state
                    .add(Arc::new(Mutex::new(i.clone())))
                    .expect("add resource")
            }),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::WitchConsumePotionEvent(data) = &event {
            self.potion = data.potion.as_ref().map(|i| peek_item(state, i));
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::WitchConsumePotionEvent(data) => Self {
                witch: consume_entity(state, &data.witch).expect("entity resource"),
                potion: data.potion.as_ref().map(|i| take_item(state, i)),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::witch_ready_potion::WitchReadyPotionEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::WitchReadyPotionEvent(WitchReadyPotionEventData {
            witch: state.add(self.witch.clone()).expect("add resource"),
            potion: self.potion.as_ref().map(|i| {
                state
                    .add(Arc::new(Mutex::new(i.clone())))
                    .expect("add resource")
            }),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::WitchReadyPotionEvent(data) = &event {
            self.potion = data.potion.as_ref().map(|i| peek_item(state, i));
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::WitchReadyPotionEvent(data) => Self {
                witch: consume_entity(state, &data.witch).expect("entity resource"),
                potion: data.potion.as_ref().map(|i| take_item(state, i)),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_portal_ready::EntityPortalReadyEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityPortalReadyEvent(EntityPortalReadyEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            portal_type: self.portal_type.clone(),
            target_world: state.add(self.target_world.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityPortalReadyEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityPortalReadyEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                portal_type: data.portal_type,
                target_world: consume_world(state, &data.target_world),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::turtle_lay_egg::TurtleLayEggEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::TurtleLayEggEvent(TurtleLayEggEventData {
            turtle: state.add(self.turtle.clone()).expect("add resource"),
            location: to_wasm_block_position(self.location),
            egg_count: self.egg_count,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::TurtleLayEggEvent(data) = &event {
            self.egg_count = data.egg_count;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::TurtleLayEggEvent(data) => Self {
                turtle: consume_entity(state, &data.turtle).expect("entity resource"),
                location: from_wasm_block_position(data.location),
                egg_count: data.egg_count,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::turtle_start_digging::TurtleStartDiggingEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::TurtleStartDiggingEvent(TurtleStartDiggingEventData {
            turtle: state.add(self.turtle.clone()).expect("add resource"),
            location: to_wasm_block_position(self.location),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::TurtleStartDiggingEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::TurtleStartDiggingEvent(data) => Self {
                turtle: consume_entity(state, &data.turtle).expect("entity resource"),
                location: from_wasm_block_position(data.location),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::turtle_go_home::TurtleGoHomeEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::TurtleGoHomeEvent(TurtleGoHomeEventData {
            turtle: state.add(self.turtle.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::TurtleGoHomeEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::TurtleGoHomeEvent(data) => Self {
                turtle: consume_entity(state, &data.turtle).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::entity_zap::EntityZapEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityZapEvent(EntityZapEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            bolt: state.add(self.bolt.clone()).expect("add resource"),
            replacement_type: self.replacement_type.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityZapEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityZapEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                bolt: consume_entity(state, &data.bolt).expect("entity resource"),
                replacement_type: data.replacement_type,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::entity_attempt_smash_attack::EntityAttemptSmashAttackEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityAttemptSmashAttackEvent(EntityAttemptSmashAttackEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            target: state.add(self.target.clone()).expect("add resource"),
            weapon: state.add(Arc::new(Mutex::new(self.weapon.clone()))).expect("add resource"),
            original_result: self.original_result,
            event_result: self.result.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityAttemptSmashAttackEvent(data) = &event {
            self.result.clone_from(&data.event_result);
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityAttemptSmashAttackEvent(data) => Self {
                entity: consume_player(state, &data.entity),
                target: consume_entity(state, &data.target).expect("entity resource"),
                weapon: take_item(state, &data.weapon),
                original_result: data.original_result,
                result: data.event_result,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_set_spawn::PlayerSetSpawnEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerSetSpawnEvent(PlayerSetSpawnEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            cause: self.cause.clone(),
            location: self.location.map(to_wasm_block_position),
            forced: self.forced,
            notify_player: self.notify_player,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerSetSpawnEvent(data) = &event {
            self.location = data.location.map(from_wasm_block_position);
            self.forced = data.forced;
            self.notify_player = data.notify_player;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerSetSpawnEvent(data) => Self {
                player: consume_player(state, &data.player),
                cause: data.cause,
                location: data.location.map(from_wasm_block_position),
                forced: data.forced,
                notify_player: data.notify_player,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_client_loaded_world::PlayerClientLoadedWorldEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerClientLoadedWorldEvent(PlayerClientLoadedWorldEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            timeout: self.timeout,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerClientLoadedWorldEvent(data) => Self {
                player: consume_player(state, &data.player),
                timeout: data.timeout,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_chunk_load::PlayerChunkLoadEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerChunkLoadEvent(PlayerChunkLoadEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            chunk_x: self.chunk_x,
            chunk_z: self.chunk_z,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerChunkLoadEvent(data) => Self {
                player: consume_player(state, &data.player),
                chunk_x: data.chunk_x,
                chunk_z: data.chunk_z,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_chunk_unload::PlayerChunkUnloadEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerChunkUnloadEvent(PlayerChunkUnloadEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            chunk_x: self.chunk_x,
            chunk_z: self.chunk_z,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerChunkUnloadEvent(data) => Self {
                player: consume_player(state, &data.player),
                chunk_x: data.chunk_x,
                chunk_z: data.chunk_z,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::block_lock_check::BlockLockCheckEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::BlockLockCheckEvent(BlockLockCheckEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            block: to_wasm_block_position(self.block),
            item: state
                .add(Arc::new(Mutex::new(self.item.clone())))
                .expect("add resource"),
            event_result: self.result.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::BlockLockCheckEvent(data) = &event {
            self.result.clone_from(&data.event_result);
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::BlockLockCheckEvent(data) => Self {
                player: consume_player(state, &data.player),
                block: from_wasm_block_position(data.block),
                item: take_item(state, &data.item),
                result: data.event_result,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_inventory_slot_change::PlayerInventorySlotChangeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerInventorySlotChangeEvent(PlayerInventorySlotChangeEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            raw_slot: self.raw_slot,
            old_item: state.add(Arc::new(Mutex::new(self.old_item.clone()))).expect("add resource"),
            new_item: state.add(Arc::new(Mutex::new(self.new_item.clone()))).expect("add resource"),
            should_trigger_advancements: self.should_trigger_advancements,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerInventorySlotChangeEvent(data) = &event {
            self.should_trigger_advancements = data.should_trigger_advancements;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerInventorySlotChangeEvent(data) => Self {
                player: consume_player(state, &data.player),
                raw_slot: data.raw_slot,
                old_item: take_item(state, &data.old_item),
                new_item: take_item(state, &data.new_item),
                should_trigger_advancements: data.should_trigger_advancements,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::inventory::item_crafted::ItemCraftedEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ItemCraftedEvent(ItemCraftedEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            crafted_item: state
                .add(Arc::new(Mutex::new(self.crafted_item.clone())))
                .expect("add resource"),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ItemCraftedEvent(data) => Self {
                player: consume_player(state, &data.player),
                crafted_item: take_item(state, &data.crafted_item),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_loom_pattern_select::PlayerLoomPatternSelectEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerLoomPatternSelectEvent(PlayerLoomPatternSelectEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            pattern: self.pattern.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerLoomPatternSelectEvent(data) = &event {
            self.pattern.clone_from(&data.pattern);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerLoomPatternSelectEvent(data) => Self {
                player: consume_player(state, &data.player),
                pattern: data.pattern,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_stonecutter_recipe_select::PlayerStonecutterRecipeSelectEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerStonecutterRecipeSelectEvent(PlayerStonecutterRecipeSelectEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            recipe: self.recipe.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerStonecutterRecipeSelectEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerStonecutterRecipeSelectEvent(data) => Self {
                player: consume_player(state, &data.player),
                recipe: data.recipe,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_advancement_criterion_grant::PlayerAdvancementCriterionGrantEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerAdvancementCriterionGrantEvent(PlayerAdvancementCriterionGrantEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            advancement: self.advancement.clone(),
            criterion: self.criterion.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerAdvancementCriterionGrantEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerAdvancementCriterionGrantEvent(data) => Self {
                player: consume_player(state, &data.player),
                advancement: data.advancement,
                criterion: data.criterion,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::ender_dragon_fireball_hit::EnderDragonFireballHitEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EnderDragonFireballHitEvent(EnderDragonFireballHitEventData {
            fireball: state.add(self.fireball.clone()).expect("add resource"),
            targets: self.targets.iter().map(|e| state.add(e.clone()).expect("add resource")).collect(),
            area_effect_cloud: state.add(self.area_effect_cloud.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EnderDragonFireballHitEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EnderDragonFireballHitEvent(data) => Self {
                fireball: consume_entity(state, &data.fireball).expect("entity resource"),
                targets: data.targets.iter().filter_map(|e| consume_entity(state, e)).collect(),
                area_effect_cloud: consume_entity(state, &data.area_effect_cloud).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::fish_hook_state_change::FishHookStateChangeEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::FishHookStateChangeEvent(FishHookStateChangeEventData {
            hook: state.add(self.hook.clone()).expect("add resource"),
            new_hook_state: self.new_hook_state.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::FishHookStateChangeEvent(data) => Self {
                hook: consume_entity(state, &data.hook).expect("entity resource"),
                new_hook_state: data.new_hook_state,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::water_bottle_splash::WaterBottleSplashEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::WaterBottleSplashEvent(WaterBottleSplashEventData {
            potion: state.add(self.potion.clone()).expect("add resource"),
            to_damage: self
                .to_damage
                .iter()
                .map(|e| state.add(e.clone()).expect("add resource"))
                .collect(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::WaterBottleSplashEvent(data) = &event {
            self.to_damage = data
                .to_damage
                .iter()
                .filter_map(|e| peek_entity(state, e))
                .collect();

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::WaterBottleSplashEvent(data) => Self {
                potion: consume_entity(state, &data.potion).expect("entity resource"),
                to_damage: data
                    .to_damage
                    .iter()
                    .filter_map(|e| consume_entity(state, e))
                    .collect(),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::async_chat_decorate::AsyncChatDecorateEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::AsyncChatDecorateEvent(AsyncChatDecorateEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            original_message: self.original_message.clone(),
            event_result: self.result.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::AsyncChatDecorateEvent(data) = &event {
            self.result.clone_from(&data.event_result);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::AsyncChatDecorateEvent(data) => Self {
                player: consume_player(state, &data.player),
                original_message: data.original_message,
                result: data.event_result,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_handshake::PlayerHandshakeEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PlayerHandshakeEvent(PlayerHandshakeEventData {
            original_handshake: self.original_handshake.clone(),
            server_hostname: self.server_hostname.clone(),
            failed: self.failed,
            fail_message: self.fail_message.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerHandshakeEvent(data) = &event {
            self.server_hostname.clone_from(&data.server_hostname);
            self.failed = data.failed;
            self.fail_message.clone_from(&data.fail_message);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerHandshakeEvent(data) => Self {
                original_handshake: data.original_handshake,
                server_hostname: data.server_hostname,
                failed: data.failed,
                fail_message: data.fail_message,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_connection_initial_configure::PlayerConnectionInitialConfigureEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PlayerConnectionInitialConfigureEvent(PlayerConnectionInitialConfigureEventData {
            name: self.name.clone(),
            uuid: self.uuid.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerConnectionInitialConfigureEvent(data) => Self {
                name: data.name,
                uuid: data.uuid,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_connection_validate_login::PlayerConnectionValidateLoginEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PlayerConnectionValidateLoginEvent(PlayerConnectionValidateLoginEventData {
            name: self.name.clone(),
            uuid: self.uuid.clone(),
            kick_message: self.kick_message.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerConnectionValidateLoginEvent(data) = &event {
            self.kick_message.clone_from(&data.kick_message);
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerConnectionValidateLoginEvent(data) => Self {
                name: data.name,
                uuid: data.uuid,
                kick_message: data.kick_message,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::async_player_connection_configure::AsyncPlayerConnectionConfigureEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::AsyncPlayerConnectionConfigureEvent(AsyncPlayerConnectionConfigureEventData {
            name: self.name.clone(),
            uuid: self.uuid.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::AsyncPlayerConnectionConfigureEvent(data) => Self {
                name: data.name,
                uuid: data.uuid,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::server::async_tab_complete::AsyncTabCompleteEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::AsyncTabCompleteEvent(AsyncTabCompleteEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            buffer: self.buffer.clone(),
            completions: self.completions.clone(),
            is_command: self.is_command,
            handled: self.handled,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::AsyncTabCompleteEvent(data) = &event {
            self.completions.clone_from(&data.completions);
            self.handled = data.handled;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::AsyncTabCompleteEvent(data) => Self {
                player: consume_player(state, &data.player),
                buffer: data.buffer,
                completions: data.completions,
                is_command: data.is_command,
                handled: data.handled,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::server::tab_complete::TabCompleteEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::TabCompleteEvent(TabCompleteEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            buffer: self.buffer.clone(),
            completions: self.completions.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::TabCompleteEvent(data) = &event {
            self.completions.clone_from(&data.completions);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::TabCompleteEvent(data) => Self {
                player: consume_player(state, &data.player),
                buffer: data.buffer,
                completions: data.completions,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::async_player_send_suggestions::AsyncPlayerSendSuggestionsEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::AsyncPlayerSendSuggestionsEvent(AsyncPlayerSendSuggestionsEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            buffer: self.buffer.clone(),
            suggestions: self.suggestions.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::AsyncPlayerSendSuggestionsEvent(data) = &event {
            self.suggestions.clone_from(&data.suggestions);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::AsyncPlayerSendSuggestionsEvent(data) => Self {
                player: consume_player(state, &data.player),
                buffer: data.buffer,
                suggestions: data.suggestions,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_use_unknown_entity::PlayerUseUnknownEntityEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerUseUnknownEntityEvent(PlayerUseUnknownEntityEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            entity_id: self.entity_id,
            attack: self.attack,
            hand: self.hand.clone(),
            clicked_position: self.clicked_position.map(to_wasm_position),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerUseUnknownEntityEvent(data) => Self {
                player: consume_player(state, &data.player),
                entity_id: data.entity_id,
                attack: data.attack,
                hand: data.hand,
                clicked_position: data.clicked_position.map(from_wasm_position),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_change_beacon_effect::PlayerChangeBeaconEffectEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerChangeBeaconEffectEvent(PlayerChangeBeaconEffectEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            beacon: to_wasm_block_position(self.beacon),
            primary: self.primary.clone(),
            secondary: self.secondary.clone(),
            consume_item: self.consume_item,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerChangeBeaconEffectEvent(data) = &event {
            self.primary.clone_from(&data.primary);
            self.secondary.clone_from(&data.secondary);
            self.consume_item = data.consume_item;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerChangeBeaconEffectEvent(data) => Self {
                player: consume_player(state, &data.player),
                beacon: from_wasm_block_position(data.beacon),
                primary: data.primary,
                secondary: data.secondary,
                consume_item: data.consume_item,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::unchecked_sign_change::UncheckedSignChangeEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::UncheckedSignChangeEvent(UncheckedSignChangeEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            location: to_wasm_block_position(self.location),
            side: self.side.clone(),
            lines: self.lines.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::UncheckedSignChangeEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::UncheckedSignChangeEvent(data) => Self {
                player: consume_player(state, &data.player),
                location: from_wasm_block_position(data.location),
                side: data.side,
                lines: data.lines,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_server_full_check::PlayerServerFullCheckEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PlayerServerFullCheckEvent(PlayerServerFullCheckEventData {
            name: self.name.clone(),
            uuid: self.uuid.clone(),
            allowed: self.allowed,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerServerFullCheckEvent(data) = &event {
            self.allowed = data.allowed;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerServerFullCheckEvent(data) => Self {
                name: data.name,
                uuid: data.uuid,
                allowed: data.allowed,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::server::profile_whitelist_verify::ProfileWhitelistVerifyEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::ProfileWhitelistVerifyEvent(ProfileWhitelistVerifyEventData {
            name: self.name.clone(),
            uuid: self.uuid.clone(),
            whitelisted: self.whitelisted,
            whitelist_enabled: self.whitelist_enabled,
            is_op: self.is_op,
            kick_message: self.kick_message.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::ProfileWhitelistVerifyEvent(data) = &event {
            self.whitelisted = data.whitelisted;
            self.kick_message.clone_from(&data.kick_message);
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::ProfileWhitelistVerifyEvent(data) => Self {
                name: data.name,
                uuid: data.uuid,
                whitelisted: data.whitelisted,
                whitelist_enabled: data.whitelist_enabled,
                is_op: data.is_op,
                kick_message: data.kick_message,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::server::remote_server_command::RemoteServerCommandEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::RemoteServerCommandEvent(RemoteServerCommandEventData {
            command: self.command.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::RemoteServerCommandEvent(data) = &event {
            self.command.clone_from(&data.command);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::RemoteServerCommandEvent(data) => Self {
                command: data.command,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::server::plugin_enable::PluginEnableEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PluginEnableEvent(PluginEnableEventData {
            plugin_name: self.plugin_name.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PluginEnableEvent(data) => Self {
                plugin_name: data.plugin_name,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::server::plugin_disable::PluginDisableEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PluginDisableEvent(PluginDisableEventData {
            plugin_name: self.plugin_name.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PluginDisableEvent(data) => Self {
                plugin_name: data.plugin_name,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_naturally_spawn_creatures::PlayerNaturallySpawnCreaturesEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerNaturallySpawnCreaturesEvent(PlayerNaturallySpawnCreaturesEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            radius: self.radius,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerNaturallySpawnCreaturesEvent(data) = &event {
            self.radius = data.radius;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerNaturallySpawnCreaturesEvent(data) => Self {
                player: consume_player(state, &data.player),
                radius: data.radius,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::block_destroy::BlockDestroyEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::BlockDestroyEvent(BlockDestroyEventData {
            block: to_wasm_block_position(self.block),
            will_drop: self.will_drop,
            play_effect: self.play_effect,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::BlockDestroyEvent(data) = &event {
            self.will_drop = data.will_drop;
            self.play_effect = data.play_effect;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::BlockDestroyEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                will_drop: data.will_drop,
                play_effect: data.play_effect,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::pre_creature_spawn::PreCreatureSpawnEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PreCreatureSpawnEvent(PreCreatureSpawnEventData {
            location: to_wasm_position(self.location),
            entity_type: self.entity_type.clone(),
            reason: self.reason.clone(),
            should_abort_spawn: self.should_abort_spawn,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PreCreatureSpawnEvent(data) = &event {
            self.should_abort_spawn = data.should_abort_spawn;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PreCreatureSpawnEvent(data) => Self {
                location: from_wasm_position(data.location),
                entity_type: data.entity_type,
                reason: data.reason,
                should_abort_spawn: data.should_abort_spawn,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::phantom_pre_spawn::PhantomPreSpawnEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PhantomPreSpawnEvent(PhantomPreSpawnEventData {
            location: to_wasm_position(self.location),
            entity: state.add(self.entity.clone()).expect("add resource"),
            should_abort_spawn: self.should_abort_spawn,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PhantomPreSpawnEvent(data) = &event {
            self.should_abort_spawn = data.should_abort_spawn;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PhantomPreSpawnEvent(data) => Self {
                location: from_wasm_position(data.location),
                entity: consume_player(state, &data.entity),
                should_abort_spawn: data.should_abort_spawn,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}
