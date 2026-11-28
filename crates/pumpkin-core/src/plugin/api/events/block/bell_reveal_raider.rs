use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `BellRevealRaiderEvent`: a ringing bell makes a raider glow.
#[cancellable]
#[derive(Event, Clone)]
pub struct BellRevealRaiderEvent {
    pub block: BlockPos,
    pub entity: Arc<dyn EntityBase>,
}
