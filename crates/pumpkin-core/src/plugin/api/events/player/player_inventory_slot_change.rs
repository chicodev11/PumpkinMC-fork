use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A slot of a player's inventory changed (Paper `PlayerInventorySlotChangeEvent`).
#[derive(Event, Clone)]
pub struct PlayerInventorySlotChangeEvent {
    pub player: Arc<Player>,
    pub raw_slot: i32,
    pub old_item: ItemStack,
    pub new_item: ItemStack,
    pub should_trigger_advancements: bool,
}

impl PlayerEvent for PlayerInventorySlotChangeEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
