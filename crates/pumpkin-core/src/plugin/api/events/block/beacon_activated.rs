use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;

/// Paper `BeaconActivatedEvent`: a beacon got a valid base.
#[derive(Event, Clone)]
pub struct BeaconActivatedEvent {
    pub block: BlockPos,
    pub levels: i32,
}
