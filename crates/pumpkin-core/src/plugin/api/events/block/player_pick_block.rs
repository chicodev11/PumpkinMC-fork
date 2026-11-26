use pumpkin_data::BlockStateId;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::player::Player;
use crate::world::World;

/// A player picks a block (middle click). `item` is what vanilla would pick (empty when the
/// block has no item, for example a custom block without one); plugins may replace it.
/// Cancelling picks nothing.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerPickBlockEvent {
    pub player: Arc<Player>,
    pub world: Arc<World>,
    pub block_pos: BlockPos,
    pub state: BlockStateId,
    pub item: ItemStack,
    /// Ctrl+pick in creative: the block entity's data is copied onto the item.
    pub include_data: bool,
}

impl PlayerPickBlockEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        world: Arc<World>,
        block_pos: BlockPos,
        state: BlockStateId,
        item: ItemStack,
        include_data: bool,
    ) -> Self {
        Self {
            player,
            world,
            block_pos,
            state,
            item,
            include_data,
            cancelled: false,
        }
    }
}
