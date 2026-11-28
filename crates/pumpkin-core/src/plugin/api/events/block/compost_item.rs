use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;

/// A hopper puts an item into a composter (Paper `CompostItemEvent`).
#[derive(Event, Clone)]
pub struct CompostItemEvent {
    pub block: BlockPos,
    pub item: ItemStack,
    pub will_raise_level: bool,
}
