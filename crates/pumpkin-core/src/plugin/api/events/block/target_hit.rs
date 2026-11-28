use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `TargetHitEvent`: a projectile hit a target block. Change `signal-strength` (0-15) or cancel.
#[cancellable]
#[derive(Event, Clone)]
pub struct TargetHitEvent {
    pub projectile: Arc<dyn EntityBase>,
    pub block: BlockPos,
    pub signal_strength: i32,
}
