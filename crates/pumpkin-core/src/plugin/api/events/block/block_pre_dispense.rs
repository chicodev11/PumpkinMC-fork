use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

/// Paper `BlockPreDispenseEvent`: a dispenser picked the item in `slot` and is about to dispense it.
#[cancellable]
#[derive(Event, Clone)]
pub struct BlockPreDispenseEvent {
    pub block: BlockPos,
    pub item: ItemStack,
    pub slot: i32,
}
