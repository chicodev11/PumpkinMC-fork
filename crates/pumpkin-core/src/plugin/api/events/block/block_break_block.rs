use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;

/// Flowing fluid washes a block away (Paper `BlockBreakBlockEvent`).
#[derive(Event, Clone)]
pub struct BlockBreakBlockEvent {
    pub block: BlockPos,
    pub source: BlockPos,
    pub drops: Vec<ItemStack>,
}
