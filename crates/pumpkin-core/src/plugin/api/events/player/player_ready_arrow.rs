use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerReadyArrowEvent`: a bow or crossbow picks an arrow. Cancel to skip it (the next is tried).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerReadyArrowEvent {
    pub player: Arc<Player>,
    pub bow: ItemStack,
    pub arrow: ItemStack,
}

impl PlayerEvent for PlayerReadyArrowEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
