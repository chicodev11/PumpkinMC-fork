use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player puts a book on a lectern (Paper `PlayerInsertLecternBookEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerInsertLecternBookEvent {
    pub player: Arc<Player>,
    pub block: BlockPos,
    pub book: ItemStack,
}

impl PlayerEvent for PlayerInsertLecternBookEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
