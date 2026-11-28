use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::world::World;

/// A world border changes size (Paper `WorldBorderBoundsChangeEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct WorldBorderBoundsChangeEvent {
    pub world: Arc<World>,
    pub change_type: String,
    pub old_size: f64,
    pub new_size: f64,
    pub duration: f64,
}
