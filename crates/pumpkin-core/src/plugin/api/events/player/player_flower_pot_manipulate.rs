use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerFlowerPotManipulateEvent`: a plant goes into (`placing`) or out of a flower pot.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerFlowerPotManipulateEvent {
    pub player: Arc<Player>,
    pub flowerpot: BlockPos,
    pub item: ItemStack,
    pub placing: bool,
}

impl PlayerEvent for PlayerFlowerPotManipulateEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
