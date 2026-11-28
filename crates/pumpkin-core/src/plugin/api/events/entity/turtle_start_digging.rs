use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// A turtle starts digging to lay eggs (Paper `TurtleStartDiggingEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct TurtleStartDiggingEvent {
    pub turtle: Arc<dyn EntityBase>,
    pub location: BlockPos,
}
