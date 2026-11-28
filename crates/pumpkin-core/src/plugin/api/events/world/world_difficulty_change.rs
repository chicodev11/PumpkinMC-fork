use pumpkin_macros::Event;
use std::sync::Arc;

use crate::world::World;

/// Paper `WorldDifficultyChangeEvent`: the difficulty changed. Read-only.
#[derive(Event, Clone)]
pub struct WorldDifficultyChangeEvent {
    pub world: Arc<World>,
    pub difficulty: String,
}
