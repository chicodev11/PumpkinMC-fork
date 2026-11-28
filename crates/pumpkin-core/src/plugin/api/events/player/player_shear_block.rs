use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player shears a block such as a pumpkin or a beehive (Paper `PlayerShearBlockEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerShearBlockEvent {
    pub player: Arc<Player>,
    pub block: BlockPos,
    pub item: ItemStack,
    pub hand: String,
    pub drops: Vec<ItemStack>,
}

impl PlayerEvent for PlayerShearBlockEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
