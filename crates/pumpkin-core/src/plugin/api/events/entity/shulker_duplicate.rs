use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A shulker hit by a shulker bullet duplicates (Paper `ShulkerDuplicateEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct ShulkerDuplicateEvent {
    pub entity: Arc<dyn EntityBase>,
    pub parent: Arc<dyn EntityBase>,
}
