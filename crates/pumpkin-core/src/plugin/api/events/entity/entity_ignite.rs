use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// An entity starts its fuse (Paper `EntityIgniteEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityIgniteEvent {
    pub entity: Arc<dyn EntityBase>,
    pub fuse_time: i32,
}
