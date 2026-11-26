//! Per-viewer item rewriting for plugins ([`ClientItemViewEvent`]).
//!
//! A Java packet serialized for one player runs with that player as the viewer; every item
//! stack written meanwhile goes through the event. Packets shared by several players are
//! serialized once per player while a handler is registered.

use std::cell::RefCell;
use std::sync::{Arc, Weak};

use pumpkin_data::item_stack::ItemStack;

use crate::entity::player::Player;
use crate::plugin::api::events::player::client_item_view::ClientItemViewEvent;
use crate::server::Server;

thread_local! {
    static VIEWER: RefCell<Option<Arc<Player>>> = const { RefCell::new(None) };
}

/// Runs `f` with `player` as the viewer of the items it serializes.
pub fn with_viewer<R>(player: Option<Arc<Player>>, f: impl FnOnce() -> R) -> R {
    let previous = VIEWER.with(|viewer| viewer.replace(player));
    let result = f();
    VIEWER.with(|viewer| *viewer.borrow_mut() = previous);
    result
}

/// Whether some plugin rewrites items per viewer, so shared packets must be built per player.
#[must_use]
pub fn is_active(server: &Server) -> bool {
    server.plugin_manager.has_handlers::<ClientItemViewEvent>()
}

/// Installs the protocol hook that fires [`ClientItemViewEvent`] for the current viewer.
pub fn install(server: &Arc<Server>) {
    let server: Weak<Server> = Arc::downgrade(server);
    pumpkin_protocol::codec::item_stack_seralizer::set_item_view_hook(Box::new(
        move |stack: &ItemStack| {
            let player = VIEWER.with(|viewer| viewer.borrow().clone())?;
            let server = server.upgrade()?;
            if !is_active(&server) {
                return None;
            }
            let mut event = ClientItemViewEvent::new(player, stack.clone());
            // A handler that sends packets of its own must not see itself as the viewer.
            with_viewer(None, || {
                server.plugin_manager.fire_blocking(&server, &mut event);
            });
            (!event.item.are_equal(stack)).then_some(event.item)
        },
    ));
}
