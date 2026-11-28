use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::player::Player;

/// A player opens a locked container (Paper `BlockLockCheckEvent`). `result` is `DEFAULT`, `ALLOW` or `DENY`.
#[derive(Event, Clone)]
pub struct BlockLockCheckEvent {
    pub player: Arc<Player>,
    pub block: BlockPos,
    pub item: ItemStack,
    pub result: String,
}
