//! Paper events added in bulk: conversions between the core events and their WIT records.

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
        AnvilDamagedEventData, BeaconActivatedEventData, BeaconDeactivatedEventData,
        BeaconEffectEventData, BellRevealRaiderEventData, BlockBreakProgressUpdateEventData,
        BlockFailedDispenseEventData, BlockPreDispenseEventData, ClientTickEndEventData,
        CreeperIgniteEventData, DragonEggFormEventData, ElderGuardianAppearanceEventData,
        EndermanAttackPlayerEventData, EntityInsideBlockEventData, EntityJumpEventData,
        EntityLoadCrossbowEventData, EntityPathfindEventData, Event, ExperienceOrbMergeEventData,
        PlayerArmSwingEventData, PlayerArmorChangeEventData,
        PlayerAttackEntityCooldownResetEventData, PlayerClientOptionsChangeEventData,
        PlayerConnectionCloseEventData, PlayerDeepSleepEventData,
        PlayerFlowerPotManipulateEventData, PlayerItemCooldownEventData,
        PlayerItemFrameChangeEventData, PlayerItemGroupCooldownEventData,
        PlayerLaunchProjectileEventData, PlayerMapFilledEventData, PlayerReadyArrowEventData,
        PlayerShieldDisableEventData, PlayerStartSpectatingEntityEventData,
        PlayerStopSpectatingEntityEventData, PlayerSwapWithEquipmentSlotEventData,
        PufferFishStateChangeEventData, ServerResourcesReloadedEventData,
        SkeletonHorseTrapEventData, TargetHitEventData, ThrownEggHatchEventData,
        VaultChangeStateEventData, WorldDifficultyChangeEventData, WorldGameRuleChangeEventData,
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

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_item_group_cooldown::PlayerItemGroupCooldownEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerItemGroupCooldownEvent(PlayerItemGroupCooldownEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            cooldown_group: self.cooldown_group.clone(),
            cooldown: self.cooldown,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerItemGroupCooldownEvent(data) = &event {
            self.cooldown = data.cooldown;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerItemGroupCooldownEvent(data) => Self {
                player: consume_player(state, &data.player),
                cooldown_group: data.cooldown_group,
                cooldown: data.cooldown,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_item_cooldown::PlayerItemCooldownEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerItemCooldownEvent(PlayerItemCooldownEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            item_type: self.item_type.clone(),
            cooldown_group: self.cooldown_group.clone(),
            cooldown: self.cooldown,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerItemCooldownEvent(data) = &event {
            self.cooldown = data.cooldown;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerItemCooldownEvent(data) => Self {
                player: consume_player(state, &data.player),
                item_type: data.item_type,
                cooldown_group: data.cooldown_group,
                cooldown: data.cooldown,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_launch_projectile::PlayerLaunchProjectileEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerLaunchProjectileEvent(PlayerLaunchProjectileEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            item: state.add(Arc::new(Mutex::new(self.item.clone()))).expect("add resource"),
            projectile: state.add(self.projectile.clone()).expect("add resource"),
            should_consume: self.should_consume,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerLaunchProjectileEvent(data) = &event {
            self.should_consume = data.should_consume;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerLaunchProjectileEvent(data) => Self {
                player: consume_player(state, &data.player),
                item: take_item(state, &data.item),
                projectile: consume_entity(state, &data.projectile).expect("entity resource"),
                should_consume: data.should_consume,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_start_spectating_entity::PlayerStartSpectatingEntityEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerStartSpectatingEntityEvent(PlayerStartSpectatingEntityEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            current_spectator_target: state.add(self.current_spectator_target.clone()).expect("add resource"),
            new_spectator_target: state.add(self.new_spectator_target.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerStartSpectatingEntityEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerStartSpectatingEntityEvent(data) => Self {
                player: consume_player(state, &data.player),
                current_spectator_target: consume_entity(state, &data.current_spectator_target).expect("entity resource"),
                new_spectator_target: consume_entity(state, &data.new_spectator_target).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_stop_spectating_entity::PlayerStopSpectatingEntityEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerStopSpectatingEntityEvent(PlayerStopSpectatingEntityEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            spectator_target: state.add(self.spectator_target.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerStopSpectatingEntityEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerStopSpectatingEntityEvent(data) => Self {
                player: consume_player(state, &data.player),
                spectator_target: consume_entity(state, &data.spectator_target).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_attack_entity_cooldown_reset::PlayerAttackEntityCooldownResetEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerAttackEntityCooldownResetEvent(PlayerAttackEntityCooldownResetEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            attacked: state.add(self.attacked.clone()).expect("add resource"),
            cooldown: self.cooldown,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerAttackEntityCooldownResetEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerAttackEntityCooldownResetEvent(data) => Self {
                player: consume_player(state, &data.player),
                attacked: consume_entity(state, &data.attacked).expect("entity resource"),
                cooldown: data.cooldown,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_armor_change::PlayerArmorChangeEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerArmorChangeEvent(PlayerArmorChangeEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            slot: self.slot.clone(),
            old_item: state
                .add(Arc::new(Mutex::new(self.old_item.clone())))
                .expect("add resource"),
            new_item: state
                .add(Arc::new(Mutex::new(self.new_item.clone())))
                .expect("add resource"),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerArmorChangeEvent(data) => Self {
                player: consume_player(state, &data.player),
                slot: data.slot,
                old_item: take_item(state, &data.old_item),
                new_item: take_item(state, &data.new_item),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_deep_sleep::PlayerDeepSleepEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerDeepSleepEvent(PlayerDeepSleepEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerDeepSleepEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerDeepSleepEvent(data) => Self {
                player: consume_player(state, &data.player),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_shield_disable::PlayerShieldDisableEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerShieldDisableEvent(PlayerShieldDisableEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            damager: self
                .damager
                .as_ref()
                .map(|e| state.add(e.clone()).expect("add resource")),
            cooldown: self.cooldown,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerShieldDisableEvent(data) = &event {
            self.cooldown = data.cooldown;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerShieldDisableEvent(data) => Self {
                player: consume_player(state, &data.player),
                damager: data.damager.as_ref().and_then(|e| consume_entity(state, e)),
                cooldown: data.cooldown,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_arm_swing::PlayerArmSwingEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerArmSwingEvent(PlayerArmSwingEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            hand: self.hand.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerArmSwingEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerArmSwingEvent(data) => Self {
                player: consume_player(state, &data.player),
                hand: data.hand,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::client_tick_end::ClientTickEndEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ClientTickEndEvent(ClientTickEndEventData {
            player: state.add(self.player.clone()).expect("add resource"),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ClientTickEndEvent(data) => Self {
                player: consume_player(state, &data.player),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_client_options_change::PlayerClientOptionsChangeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerClientOptionsChangeEvent(PlayerClientOptionsChangeEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            locale: self.locale.clone(),
            view_distance: self.view_distance,
            chat_visibility: self.chat_visibility.clone(),
            chat_colors: self.chat_colors,
            skin_parts: self.skin_parts,
            main_hand: self.main_hand.clone(),
            text_filtering: self.text_filtering,
            allow_server_listings: self.allow_server_listings,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerClientOptionsChangeEvent(data) => Self {
                player: consume_player(state, &data.player),
                locale: data.locale,
                view_distance: data.view_distance,
                chat_visibility: data.chat_visibility,
                chat_colors: data.chat_colors,
                skin_parts: data.skin_parts,
                main_hand: data.main_hand,
                text_filtering: data.text_filtering,
                allow_server_listings: data.allow_server_listings,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_swap_with_equipment_slot::PlayerSwapWithEquipmentSlotEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerSwapWithEquipmentSlotEvent(PlayerSwapWithEquipmentSlotEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            hand: self.hand.clone(),
            slot: self.slot.clone(),
            item: state.add(Arc::new(Mutex::new(self.item.clone()))).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerSwapWithEquipmentSlotEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerSwapWithEquipmentSlotEvent(data) => Self {
                player: consume_player(state, &data.player),
                hand: data.hand,
                slot: data.slot,
                item: take_item(state, &data.item),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_map_filled::PlayerMapFilledEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerMapFilledEvent(PlayerMapFilledEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            original_item: state
                .add(Arc::new(Mutex::new(self.original_item.clone())))
                .expect("add resource"),
            created_map: state
                .add(Arc::new(Mutex::new(self.created_map.clone())))
                .expect("add resource"),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerMapFilledEvent(data) = &event {
            self.created_map = peek_item(state, &data.created_map);
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerMapFilledEvent(data) => Self {
                player: consume_player(state, &data.player),
                original_item: take_item(state, &data.original_item),
                created_map: take_item(state, &data.created_map),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_flower_pot_manipulate::PlayerFlowerPotManipulateEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerFlowerPotManipulateEvent(PlayerFlowerPotManipulateEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            flowerpot: to_wasm_block_position(self.flowerpot),
            item: state.add(Arc::new(Mutex::new(self.item.clone()))).expect("add resource"),
            placing: self.placing,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerFlowerPotManipulateEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerFlowerPotManipulateEvent(data) => Self {
                player: consume_player(state, &data.player),
                flowerpot: from_wasm_block_position(data.flowerpot),
                item: take_item(state, &data.item),
                placing: data.placing,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_connection_close::PlayerConnectionCloseEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::PlayerConnectionCloseEvent(PlayerConnectionCloseEventData {
            player_uuid: self.player_uuid.clone(),
            player_name: self.player_name.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerConnectionCloseEvent(data) => Self {
                player_uuid: data.player_uuid,
                player_name: data.player_name,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::player::player_item_frame_change::PlayerItemFrameChangeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerItemFrameChangeEvent(PlayerItemFrameChangeEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            item_frame: state.add(self.item_frame.clone()).expect("add resource"),
            item: state.add(Arc::new(Mutex::new(self.item.clone()))).expect("add resource"),
            action: self.action.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerItemFrameChangeEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerItemFrameChangeEvent(data) => Self {
                player: consume_player(state, &data.player),
                item_frame: consume_entity(state, &data.item_frame).expect("entity resource"),
                item: take_item(state, &data.item),
                action: data.action,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_ready_arrow::PlayerReadyArrowEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerReadyArrowEvent(PlayerReadyArrowEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            bow: state
                .add(Arc::new(Mutex::new(self.bow.clone())))
                .expect("add resource"),
            arrow: state
                .add(Arc::new(Mutex::new(self.arrow.clone())))
                .expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PlayerReadyArrowEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerReadyArrowEvent(data) => Self {
                player: consume_player(state, &data.player),
                bow: take_item(state, &data.bow),
                arrow: take_item(state, &data.arrow),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::anvil_damaged::AnvilDamagedEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::AnvilDamagedEvent(AnvilDamagedEventData {
            player: state.add(self.player.clone()).expect("add resource"),
            block: to_wasm_block_position(self.block),
            damage_state: self.damage_state.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::AnvilDamagedEvent(data) = &event {
            self.damage_state.clone_from(&data.damage_state);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::AnvilDamagedEvent(data) => Self {
                player: consume_player(state, &data.player),
                block: from_wasm_block_position(data.block),
                damage_state: data.damage_state,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::creeper_ignite::CreeperIgniteEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::CreeperIgniteEvent(CreeperIgniteEventData {
            creeper: state.add(self.creeper.clone()).expect("add resource"),
            ignited: self.ignited,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::CreeperIgniteEvent(data) = &event {
            self.ignited = data.ignited;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::CreeperIgniteEvent(data) => Self {
                creeper: consume_entity(state, &data.creeper).expect("entity resource"),
                ignited: data.ignited,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::puffer_fish_state_change::PufferFishStateChangeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PufferFishStateChangeEvent(PufferFishStateChangeEventData {
            pufferfish: state.add(self.pufferfish.clone()).expect("add resource"),
            new_puff_state: self.new_puff_state,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::PufferFishStateChangeEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PufferFishStateChangeEvent(data) => Self {
                pufferfish: consume_entity(state, &data.pufferfish).expect("entity resource"),
                new_puff_state: data.new_puff_state,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::enderman_attack_player::EndermanAttackPlayerEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EndermanAttackPlayerEvent(EndermanAttackPlayerEventData {
            enderman: state.add(self.enderman.clone()).expect("add resource"),
            player: state.add(self.player.clone()).expect("add resource"),
            should_attack: self.should_attack,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EndermanAttackPlayerEvent(data) = &event {
            self.should_attack = data.should_attack;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EndermanAttackPlayerEvent(data) => Self {
                enderman: consume_entity(state, &data.enderman).expect("entity resource"),
                player: consume_player(state, &data.player),
                should_attack: data.should_attack,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::thrown_egg_hatch::ThrownEggHatchEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ThrownEggHatchEvent(ThrownEggHatchEventData {
            egg: state.add(self.egg.clone()).expect("add resource"),
            hatching: self.hatching,
            num_hatches: self.num_hatches,
            hatching_type: self.hatching_type.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::ThrownEggHatchEvent(data) = &event {
            self.hatching = data.hatching;
            self.num_hatches = data.num_hatches;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ThrownEggHatchEvent(data) => Self {
                egg: consume_entity(state, &data.egg).expect("entity resource"),
                hatching: data.hatching,
                num_hatches: data.num_hatches,
                hatching_type: data.hatching_type,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::elder_guardian_appearance::ElderGuardianAppearanceEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ElderGuardianAppearanceEvent(ElderGuardianAppearanceEventData {
            guardian: state.add(self.guardian.clone()).expect("add resource"),
            affected_player: state.add(self.affected_player.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::ElderGuardianAppearanceEvent(data) = &event {

            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ElderGuardianAppearanceEvent(data) => Self {
                guardian: consume_entity(state, &data.guardian).expect("entity resource"),
                affected_player: consume_player(state, &data.affected_player),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::entity::entity_jump::EntityJumpEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityJumpEvent(EntityJumpEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityJumpEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityJumpEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::experience_orb_merge::ExperienceOrbMergeEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ExperienceOrbMergeEvent(ExperienceOrbMergeEventData {
            merge_target: state.add(self.merge_target.clone()).expect("add resource"),
            merge_source: state.add(self.merge_source.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::ExperienceOrbMergeEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ExperienceOrbMergeEvent(data) => Self {
                merge_target: consume_entity(state, &data.merge_target).expect("entity resource"),
                merge_source: consume_entity(state, &data.merge_source).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_pathfind::EntityPathfindEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityPathfindEvent(EntityPathfindEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            location: to_wasm_position(self.location),
            target_entity: self
                .target_entity
                .as_ref()
                .map(|e| state.add(e.clone()).expect("add resource")),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityPathfindEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityPathfindEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                location: from_wasm_position(data.location),
                target_entity: data
                    .target_entity
                    .as_ref()
                    .and_then(|e| consume_entity(state, e)),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::entity_inside_block::EntityInsideBlockEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityInsideBlockEvent(EntityInsideBlockEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            block: to_wasm_block_position(self.block),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityInsideBlockEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityInsideBlockEvent(data) => Self {
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                block: from_wasm_block_position(data.block),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::entity::skeleton_horse_trap::SkeletonHorseTrapEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::SkeletonHorseTrapEvent(SkeletonHorseTrapEventData {
            horse: state.add(self.horse.clone()).expect("add resource"),
            eligible_players: self
                .eligible_players
                .iter()
                .map(|e| state.add(e.clone()).expect("add resource"))
                .collect(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::SkeletonHorseTrapEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::SkeletonHorseTrapEvent(data) => Self {
                horse: consume_entity(state, &data.horse).expect("entity resource"),
                eligible_players: data
                    .eligible_players
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
    for pumpkin_core::plugin::api::events::entity::entity_load_crossbow::EntityLoadCrossbowEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityLoadCrossbowEvent(EntityLoadCrossbowEventData {
            entity: state.add(self.entity.clone()).expect("add resource"),
            crossbow: state
                .add(Arc::new(Mutex::new(self.crossbow.clone())))
                .expect("add resource"),
            hand: self.hand.clone(),
            consume_item: self.consume_item,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::EntityLoadCrossbowEvent(data) = &event {
            self.consume_item = data.consume_item;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::EntityLoadCrossbowEvent(data) => Self {
                entity: consume_player(state, &data.entity),
                crossbow: take_item(state, &data.crossbow),
                hand: data.hand,
                consume_item: data.consume_item,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::beacon_effect::BeaconEffectEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::BeaconEffectEvent(BeaconEffectEventData {
            block: to_wasm_block_position(self.block),
            player: state.add(self.player.clone()).expect("add resource"),
            effect_type: self.effect_type.clone(),
            amplifier: self.amplifier,
            duration: self.duration,
            is_primary: self.is_primary,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::BeaconEffectEvent(data) = &event {
            self.effect_type.clone_from(&data.effect_type);
            self.amplifier = data.amplifier;
            self.duration = data.duration;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::BeaconEffectEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                player: consume_player(state, &data.player),
                effect_type: data.effect_type,
                amplifier: data.amplifier,
                duration: data.duration,
                is_primary: data.is_primary,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::beacon_activated::BeaconActivatedEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::BeaconActivatedEvent(BeaconActivatedEventData {
            block: to_wasm_block_position(self.block),
            levels: self.levels,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::BeaconActivatedEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                levels: data.levels,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::beacon_deactivated::BeaconDeactivatedEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::BeaconDeactivatedEvent(BeaconDeactivatedEventData {
            block: to_wasm_block_position(self.block),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::BeaconDeactivatedEvent(data) => Self {
                block: from_wasm_block_position(data.block),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::block::target_hit::TargetHitEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::TargetHitEvent(TargetHitEventData {
            projectile: state.add(self.projectile.clone()).expect("add resource"),
            block: to_wasm_block_position(self.block),
            signal_strength: self.signal_strength,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::TargetHitEvent(data) = &event {
            self.signal_strength = data.signal_strength;
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::TargetHitEvent(data) => Self {
                projectile: consume_entity(state, &data.projectile).expect("entity resource"),
                block: from_wasm_block_position(data.block),
                signal_strength: data.signal_strength,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::block_pre_dispense::BlockPreDispenseEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::BlockPreDispenseEvent(BlockPreDispenseEventData {
            block: to_wasm_block_position(self.block),
            item: state
                .add(Arc::new(Mutex::new(self.item.clone())))
                .expect("add resource"),
            slot: self.slot,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::BlockPreDispenseEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::BlockPreDispenseEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                item: take_item(state, &data.item),
                slot: data.slot,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::block_failed_dispense::BlockFailedDispenseEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::BlockFailedDispenseEvent(BlockFailedDispenseEventData {
            block: to_wasm_block_position(self.block),
            should_play_effect: self.should_play_effect,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::BlockFailedDispenseEvent(data) = &event {
            self.should_play_effect = data.should_play_effect;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::BlockFailedDispenseEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                should_play_effect: data.should_play_effect,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::vault_change_state::VaultChangeStateEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::VaultChangeStateEvent(VaultChangeStateEventData {
            block: to_wasm_block_position(self.block),
            current_state: self.current_state.clone(),
            new_state: self.new_state.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::VaultChangeStateEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::VaultChangeStateEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                current_state: data.current_state,
                new_state: data.new_state,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::block::block_break_progress_update::BlockBreakProgressUpdateEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::BlockBreakProgressUpdateEvent(BlockBreakProgressUpdateEventData {
            block: to_wasm_block_position(self.block),
            progress: self.progress,
            entity: self.entity.as_ref().map(|e| state.add(e.clone()).expect("add resource")),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::BlockBreakProgressUpdateEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                progress: data.progress,
                entity: data.entity.as_ref().and_then(|e| consume_entity(state, e)),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::dragon_egg_form::DragonEggFormEvent
{
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::DragonEggFormEvent(DragonEggFormEventData {
            block: to_wasm_block_position(self.block),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::DragonEggFormEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::DragonEggFormEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::block::bell_reveal_raider::BellRevealRaiderEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::BellRevealRaiderEvent(BellRevealRaiderEventData {
            block: to_wasm_block_position(self.block),
            entity: state.add(self.entity.clone()).expect("add resource"),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::BellRevealRaiderEvent(data) = &event {
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::BellRevealRaiderEvent(data) => Self {
                block: from_wasm_block_position(data.block),
                entity: consume_entity(state, &data.entity).expect("entity resource"),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::world::world_difficulty_change::WorldDifficultyChangeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::WorldDifficultyChangeEvent(WorldDifficultyChangeEventData {
            event_world: state.add(self.world.clone()).expect("add resource"),
            difficulty: self.difficulty.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::WorldDifficultyChangeEvent(data) => Self {
                world: consume_world(state, &data.event_world),
                difficulty: data.difficulty,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::world::world_game_rule_change::WorldGameRuleChangeEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::WorldGameRuleChangeEvent(WorldGameRuleChangeEventData {
            event_world: state.add(self.world.clone()).expect("add resource"),
            game_rule: self.game_rule.clone(),
            value: self.value.clone(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::WorldGameRuleChangeEvent(data) = &event {
            self.value.clone_from(&data.value);
            self.cancelled = data.cancelled;
        }
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::WorldGameRuleChangeEvent(data) => Self {
                world: consume_world(state, &data.event_world),
                game_rule: data.game_rule,
                value: data.value,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for pumpkin_core::plugin::api::events::server::server_resources_reloaded::ServerResourcesReloadedEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::ServerResourcesReloadedEvent(ServerResourcesReloadedEventData {
            cause: self.cause.clone(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::ServerResourcesReloadedEvent(data) => Self {
                cause: data.cause,
            },
            _ => panic!("unexpected event type"),
        }
    }
}
