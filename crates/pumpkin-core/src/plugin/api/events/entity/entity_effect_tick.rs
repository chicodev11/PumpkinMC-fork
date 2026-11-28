use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// An effect ticks on an entity (Paper `EntityEffectTickEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityEffectTickEvent {
    pub entity: Arc<dyn EntityBase>,
    pub effect_type: String,
    pub amplifier: i32,
}
