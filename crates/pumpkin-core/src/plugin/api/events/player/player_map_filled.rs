use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerMapFilledEvent`: an empty map was used. Change `created-map` to give another item.
#[derive(Event, Clone)]
pub struct PlayerMapFilledEvent {
    pub player: Arc<Player>,
    pub original_item: ItemStack,
    pub created_map: ItemStack,
}

impl PlayerEvent for PlayerMapFilledEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
