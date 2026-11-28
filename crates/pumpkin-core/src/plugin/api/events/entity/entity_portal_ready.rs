use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;
use crate::world::World;

/// An entity has stood in a nether portal long enough to travel (Paper `EntityPortalReadyEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityPortalReadyEvent {
    pub entity: Arc<dyn EntityBase>,
    pub portal_type: String,
    pub target_world: Arc<World>,
}
