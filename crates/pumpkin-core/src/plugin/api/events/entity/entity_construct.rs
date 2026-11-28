use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// A golem or wither is built from blocks (Paper `EntityConstructEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityConstructEvent {
    pub entity: Arc<dyn EntityBase>,
    pub blocks: Vec<BlockPos>,
}
