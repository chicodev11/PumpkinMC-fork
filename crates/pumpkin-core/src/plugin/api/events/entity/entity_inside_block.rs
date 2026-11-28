use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `EntityInsideBlockEvent`: an entity is inside a block that acts on it (fire, cobweb, portals...).
/// Fired every tick while inside, so keep handlers quick.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityInsideBlockEvent {
    pub entity: Arc<dyn EntityBase>,
    pub block: BlockPos,
}
