use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// An entity pushes into another (Paper `EntityCollideWithEntityEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityCollideWithEntityEvent {
    pub entity: Arc<dyn EntityBase>,
    pub collided: Arc<dyn EntityBase>,
}
