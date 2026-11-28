use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A slime wanders (Paper `SlimeWanderEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct SlimeWanderEvent {
    pub slime: Arc<dyn EntityBase>,
}
