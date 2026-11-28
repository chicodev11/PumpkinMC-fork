use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `EntityJumpEvent`: a living entity other than a player jumps.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityJumpEvent {
    pub entity: Arc<dyn EntityBase>,
}
