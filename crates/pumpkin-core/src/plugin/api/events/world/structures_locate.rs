use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::world::World;

/// Structures are searched for, for example by `/locate structure` (Paper `StructuresLocateEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct StructuresLocateEvent {
    pub world: Arc<World>,
    pub origin: BlockPos,
    pub structures: Vec<String>,
    pub radius: i32,
    pub find_unexplored: bool,
}
