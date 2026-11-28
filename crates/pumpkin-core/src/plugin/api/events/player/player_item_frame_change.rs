use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Paper `PlayerItemFrameChangeEvent`: a player puts an item into (`PLACE`) or turns (`ROTATE`) an item frame.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerItemFrameChangeEvent {
    pub player: Arc<Player>,
    pub item_frame: Arc<dyn EntityBase>,
    pub item: ItemStack,
    pub action: String,
}

impl PlayerEvent for PlayerItemFrameChangeEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
