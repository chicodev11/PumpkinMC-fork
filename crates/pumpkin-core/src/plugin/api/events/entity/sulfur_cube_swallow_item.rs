use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;
use crate::entity::player::Player;

/// A sulfur cube swallows an item from a player (Paper `SulfurCubeSwallowItemEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct SulfurCubeSwallowItemEvent {
    pub sulfur_cube: Arc<dyn EntityBase>,
    pub player: Arc<Player>,
    pub old_item: ItemStack,
    pub new_item: ItemStack,
}
