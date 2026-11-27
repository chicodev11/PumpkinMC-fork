use crate::entity::EntityBase;
use crate::world::World;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

/// An event that occurs when an entity is spawned into a world.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntitySpawnEvent {
    /// The ID of the spawned entity.
    pub entity_id: i32,

    /// The registry name of the entity type.
    pub entity_type: String,

    /// The position where the entity is spawned.
    pub position: Vector3<f64>,

    /// The world in which the entity is spawned.
    pub world: Arc<World>,

    /// Bukkit `getEntity`: the entity about to be added (`None` only when rebuilt from a
    /// v0.1 plugin).
    pub entity: Option<Arc<dyn EntityBase>>,
}

impl EntitySpawnEvent {
    #[must_use]
    pub fn new(entity: Arc<dyn EntityBase>, world: Arc<World>) -> Self {
        let base = entity.get_entity();
        Self {
            entity_id: base.entity_id,
            entity_type: base.entity_type.id.to_string(),
            position: base.pos.load(),
            world,
            entity: Some(entity),
            cancelled: false,
        }
    }
}
