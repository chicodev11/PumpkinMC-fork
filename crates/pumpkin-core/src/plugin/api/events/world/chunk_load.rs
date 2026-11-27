use crate::world::World;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector2::Vector2;
use std::sync::Arc;

/// Bukkit `ChunkLoadEvent`: a chunk was loaded into a world.
///
/// Read from disk or newly generated. Fired on the tick thread once the chunk is in memory;
/// cancelling it does not keep the chunk from loading (as on Paper).
#[cancellable]
#[derive(Event, Clone)]
pub struct ChunkLoad {
    /// The world in which the chunk was loaded.
    pub world: Arc<World>,

    /// Chunk coordinates.
    pub chunk_pos: Vector2<i32>,

    /// Bukkit `isNewChunk`: the chunk was generated now rather than read from disk.
    pub is_new: bool,
}

impl ChunkLoad {
    #[must_use]
    pub const fn new(world: Arc<World>, chunk_pos: Vector2<i32>, is_new: bool) -> Self {
        Self {
            world,
            chunk_pos,
            is_new,
            cancelled: false,
        }
    }
}
