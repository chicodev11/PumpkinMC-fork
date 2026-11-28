use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `SkeletonHorseTrapEvent`: a skeleton trap horse is set off by nearby players.
#[cancellable]
#[derive(Event, Clone)]
pub struct SkeletonHorseTrapEvent {
    pub horse: Arc<dyn EntityBase>,
    pub eligible_players: Vec<Arc<dyn EntityBase>>,
}
