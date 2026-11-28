use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::player::Player;

/// Paper `EntityLoadCrossbowEvent`: a crossbow is loaded. Set `consume-item` to false to keep the ammo.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityLoadCrossbowEvent {
    pub entity: Arc<Player>,
    pub crossbow: ItemStack,
    pub hand: String,
    pub consume_item: bool,
}
