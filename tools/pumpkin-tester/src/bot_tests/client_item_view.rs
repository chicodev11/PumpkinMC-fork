use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pumpkin_core::net::ClientPlatform;
use pumpkin_core::net::java::JavaClient;
use pumpkin_core::plugin::api::events::EventPriority;
use pumpkin_core::plugin::api::events::player::client_item_view::ClientItemViewEvent;
use pumpkin_core::plugin::{BoxFuture, EventHandler};
use pumpkin_core::server::Server;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::CSetPlayerInventory;

/// Shows diamonds as emeralds while `active`; handlers can't be unregistered, so the later
/// tests run with it switched off.
struct DiamondToEmerald {
    active: AtomicBool,
}

impl EventHandler<ClientItemViewEvent> for DiamondToEmerald {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut ClientItemViewEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if self.active.load(Ordering::Relaxed) && event.item.item.id == Item::DIAMOND.id {
                event.item = ItemStack::new(event.item.item_count, &Item::EMERALD);
            }
        })
    }
}

/// `ClientItemViewEvent` rewrites what one player's packets carry, and only theirs.
pub fn run_client_item_view_test(
    server: &Arc<Server>,
    bot_name: &str,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let player = server
        .get_player_by_name(bot_name)
        .ok_or("missing bot for the client item view test")?;
    let ClientPlatform::Java(client) = player.client.as_ref() else {
        return Err("Expected a Java player for the client item view test".into());
    };
    let stack = ItemStackSerializer(Cow::Owned(ItemStack::new(3, &Item::DIAMOND)));
    let packet = CSetPlayerInventory::new(VarInt(0), &stack);
    let before = client
        .serialize_packet(&packet)
        .map_err(|e| e.to_string())?;
    let handler = Arc::new(DiamondToEmerald {
        active: AtomicBool::new(true),
    });
    server.plugin_manager.register::<ClientItemViewEvent, _>(
        handler.clone(),
        EventPriority::Normal,
        true,
    );
    let viewed = client
        .serialize_packet(&packet)
        .map_err(|e| e.to_string())?;
    let shared =
        JavaClient::serialize_packet_for_version(&packet, pumpkin_data::packet::CURRENT_MC_VERSION)
            .map_err(|e| e.to_string())?;
    handler.active.store(false, Ordering::Relaxed);
    if before == viewed || viewed == shared || before != shared {
        return Err(format!(
            "ClientItemViewEvent did not rewrite the viewer's packet only: {before:02x?} {viewed:02x?} {shared:02x?}"
        ));
    }
    summary_lines.push("  client item view: rewritten for the viewer only".into());
    Ok(())
}
