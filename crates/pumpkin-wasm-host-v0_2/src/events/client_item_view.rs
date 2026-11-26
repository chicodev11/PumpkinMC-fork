use std::sync::Arc;

use pumpkin_core::plugin::api::events::player::client_item_view::ClientItemViewEvent;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_wasm_host_common::state::PluginHostState;
use tokio::sync::Mutex;
use wasmtime::component::Resource;

use crate::{
    events::{ToFromWasmEvent, consume_player},
    pumpkin::plugin::event::{ClientItemViewEventData, Event},
};

fn consume_item_stack(
    state: &mut PluginHostState,
    item: &Resource<crate::pumpkin::plugin::item_stack::ItemStack>,
) -> ItemStack {
    let mutex = state
        .resource_table
        .delete::<Arc<Mutex<ItemStack>>>(Resource::new_own(item.rep()))
        .expect("invalid item stack resource handle");
    mutex.try_lock().expect("lock item stack").clone()
}

impl ToFromWasmEvent for ClientItemViewEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");
        let item = state
            .add(Arc::new(Mutex::new(self.item.clone())))
            .expect("failed to add item stack resource");
        Event::ClientItemViewEvent(ClientItemViewEventData { player, item })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ClientItemViewEvent(data) => Self {
                player: consume_player(state, &data.player),
                item: consume_item_stack(state, &data.item),
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent
    for pumpkin_core::plugin::api::events::player::player_cookie_response::PlayerCookieResponseEvent
{
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");
        Event::PlayerCookieResponseEvent(
            crate::pumpkin::plugin::event::PlayerCookieResponseEventData {
                player,
                key: self.key.clone(),
                payload: self.payload.clone(),
            },
        )
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PlayerCookieResponseEvent(data) => Self {
                player: consume_player(state, &data.player),
                key: data.key,
                payload: data.payload,
            },
            _ => panic!("unexpected event type"),
        }
    }
}
