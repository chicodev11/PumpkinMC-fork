use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player turns a page of a book on a lectern (Paper `PlayerLecternPageChangeEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerLecternPageChangeEvent {
    pub player: Arc<Player>,
    pub lectern: BlockPos,
    pub book: ItemStack,
    pub page_turn: String,
    pub old_page: i32,
    pub new_page: i32,
}

impl PlayerEvent for PlayerLecternPageChangeEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
