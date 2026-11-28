use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A turtle heads home (Paper `TurtleGoHomeEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct TurtleGoHomeEvent {
    pub turtle: Arc<dyn EntityBase>,
}
