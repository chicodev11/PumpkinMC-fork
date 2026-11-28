use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// Lightning turns an entity into another (Paper `EntityZapEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityZapEvent {
    pub entity: Arc<dyn EntityBase>,
    pub bolt: Arc<dyn EntityBase>,
    pub replacement_type: String,
}
