use crate::world::World;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector2::Vector2;
use std::sync::Arc;

/// A chunk was handed to the writer to be saved. Fired on the tick thread after the save was
/// queued, so cancelling it does not stop the save.
#[cancellable]
#[derive(Event, Clone)]
pub struct ChunkSave {
    /// The world the chunk belongs to.
    pub world: Arc<World>,

    /// Chunk coordinates.
    pub chunk_pos: Vector2<i32>,
}

impl ChunkSave {
    #[must_use]
    pub const fn new(world: Arc<World>, chunk_pos: Vector2<i32>) -> Self {
        Self {
            world,
            chunk_pos,
            cancelled: false,
        }
    }
}
