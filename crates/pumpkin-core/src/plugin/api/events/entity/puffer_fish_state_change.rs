use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `PufferFishStateChangeEvent`: a pufferfish puffs up or down (0 small, 1 mid, 2 full).
#[cancellable]
#[derive(Event, Clone)]
pub struct PufferFishStateChangeEvent {
    pub pufferfish: Arc<dyn EntityBase>,
    pub new_puff_state: i32,
}
