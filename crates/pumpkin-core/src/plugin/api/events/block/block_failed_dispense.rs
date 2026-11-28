use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;

/// Paper `BlockFailedDispenseEvent`: a dispenser or dropper was powered while empty.
#[derive(Event, Clone)]
pub struct BlockFailedDispenseEvent {
    pub block: BlockPos,
    pub should_play_effect: bool,
}
