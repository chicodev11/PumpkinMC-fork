use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::EntityBase;

/// A fishing hook changes state (Paper `FishHookStateChangeEvent`).
#[derive(Event, Clone)]
pub struct FishHookStateChangeEvent {
    pub hook: Arc<dyn EntityBase>,
    pub new_hook_state: String,
}
