use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// A turtle lays eggs (Paper `TurtleLayEggEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct TurtleLayEggEvent {
    pub turtle: Arc<dyn EntityBase>,
    pub location: BlockPos,
    pub egg_count: i32,
}
