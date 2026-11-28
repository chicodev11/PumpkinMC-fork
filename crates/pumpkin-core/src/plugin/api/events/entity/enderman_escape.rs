use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// An enderman teleports away (Paper `EndermanEscapeEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EndermanEscapeEvent {
    pub enderman: Arc<dyn EntityBase>,
    pub reason: String,
}
