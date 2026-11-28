use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

/// A block is destroyed without a player, for example by physics (Paper `BlockDestroyEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct BlockDestroyEvent {
    pub block: BlockPos,
    pub will_drop: bool,
    pub play_effect: bool,
}
