use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A tamed animal's death message is sent to its owner (Paper `TameableDeathMessageEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct TameableDeathMessageEvent {
    pub entity: Arc<dyn EntityBase>,
    pub death_message: String,
}
