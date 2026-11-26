use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::player::Player;

use super::PlayerEvent;

/// Fired for every item stack the server writes into a packet for one Java player
/// (inventories, held and worn items, item frames, dropped items, trades, ...).
///
/// Setting `item` changes only what that player's client shows; the server keeps the real
/// stack (CraftEngine-style client items: names, lore, `item_model`). Fired only while a
/// handler is registered, from whatever thread serializes the packet, so handlers must be
/// quick. A client sends what it sees in clicks, so a rewritten stack makes the server resend
/// that slot after a click, as on Paper.
#[derive(Event, Clone)]
pub struct ClientItemViewEvent {
    /// The player the packet is for.
    pub player: Arc<Player>,
    /// The stack as the client will see it.
    pub item: ItemStack,
}

impl ClientItemViewEvent {
    #[must_use]
    pub const fn new(player: Arc<Player>, item: ItemStack) -> Self {
        Self { player, item }
    }
}

impl PlayerEvent for ClientItemViewEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
