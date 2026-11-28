use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `BlockBreakProgressUpdateEvent`: the crack progress (0-1) shown on a block changed.
#[derive(Event, Clone)]
pub struct BlockBreakProgressUpdateEvent {
    pub block: BlockPos,
    pub progress: f32,
    pub entity: Option<Arc<dyn EntityBase>>,
}
