use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A slime targets an entity (Paper `SlimeTargetLivingEntityEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct SlimeTargetLivingEntityEvent {
    pub slime: Arc<dyn EntityBase>,
    pub target: Arc<dyn EntityBase>,
}
