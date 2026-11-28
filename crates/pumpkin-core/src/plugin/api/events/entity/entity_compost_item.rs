use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// An entity puts an item into a composter (Paper `EntityCompostItemEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityCompostItemEvent {
    pub entity: Arc<dyn EntityBase>,
    pub block: BlockPos,
    pub item: ItemStack,
    pub will_raise_level: bool,
}
