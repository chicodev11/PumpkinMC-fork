use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerArmorChangeEvent`: an armor slot changed. Read-only.
#[derive(Event, Clone)]
pub struct PlayerArmorChangeEvent {
    pub player: Arc<Player>,
    pub slot: String,
    pub old_item: ItemStack,
    pub new_item: ItemStack,
}

impl PlayerEvent for PlayerArmorChangeEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
