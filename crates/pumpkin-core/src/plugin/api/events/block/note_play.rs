use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use pumpkin_util::math::position::BlockPos;

use crate::world::World;

/// An event that occurs when a note block plays.
#[cancellable]
#[derive(Event, Clone)]
pub struct NotePlayEvent {
    /// The world the note block is in.
    pub world: Arc<World>,
    pub block_pos: BlockPos,
    pub instrument: String,
    pub note: u8,
}

impl NotePlayEvent {
    #[must_use]
    pub const fn new(world: Arc<World>, block_pos: BlockPos, instrument: String, note: u8) -> Self {
        Self {
            world,
            block_pos,
            instrument,
            note,
            cancelled: false,
        }
    }
}
