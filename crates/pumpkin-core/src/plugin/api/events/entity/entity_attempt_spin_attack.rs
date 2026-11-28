use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// An entity in a riptide spin hits another (Paper `EntityAttemptSpinAttackEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityAttemptSpinAttackEvent {
    pub entity: Arc<dyn EntityBase>,
    pub target: Arc<dyn EntityBase>,
}
