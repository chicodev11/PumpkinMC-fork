use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::player::Player;

/// A player crafted an item (Paper `ItemCraftedEvent`).
#[derive(Event, Clone)]
pub struct ItemCraftedEvent {
    pub player: Arc<Player>,
    pub crafted_item: ItemStack,
}
