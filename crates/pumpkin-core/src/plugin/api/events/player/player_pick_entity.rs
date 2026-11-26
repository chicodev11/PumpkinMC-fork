use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::player::Player;

/// A player picks an entity (middle click), Paper `PlayerPickEntityEvent`.
///
/// `item` is what vanilla would pick (the spawn egg, empty when the entity has none, for example an
/// interaction or display entity used as furniture); plugins may replace it. Cancelling picks
/// nothing.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerPickEntityEvent {
    pub player: Arc<Player>,
    pub entity_id: i32,
    pub item: ItemStack,
    /// Ctrl+pick in creative.
    pub include_data: bool,
}

impl PlayerPickEntityEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        entity_id: i32,
        item: ItemStack,
        include_data: bool,
    ) -> Self {
        Self {
            player,
            entity_id,
            item,
            include_data,
            cancelled: false,
        }
    }
}
