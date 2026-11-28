use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::EntityBase;
use crate::world::World;

/// Paper `EntityAddToWorldEvent`: an entity was added to a world, spawned or loaded with its
/// chunk. Read-only.
#[derive(Event, Clone)]
pub struct EntityAddToWorldEvent {
    pub entity: Arc<dyn EntityBase>,
    pub world: Arc<World>,
}
