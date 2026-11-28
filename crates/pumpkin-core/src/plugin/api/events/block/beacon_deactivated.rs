use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;

/// Paper `BeaconDeactivatedEvent`: a beacon lost its base.
#[derive(Event, Clone)]
pub struct BeaconDeactivatedEvent {
    pub block: BlockPos,
}
