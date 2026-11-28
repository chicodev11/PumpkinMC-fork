use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A slime swims (Paper `SlimeSwimEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct SlimeSwimEvent {
    pub slime: Arc<dyn EntityBase>,
}
