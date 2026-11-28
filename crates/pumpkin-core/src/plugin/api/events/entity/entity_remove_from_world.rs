use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::EntityBase;
use crate::world::World;

/// Paper `EntityRemoveFromWorldEvent`: an entity left a world, removed or unloaded with its
/// chunk. Read-only.
#[derive(Event, Clone)]
pub struct EntityRemoveFromWorldEvent {
    pub entity: Arc<dyn EntityBase>,
    pub world: Arc<World>,
}
