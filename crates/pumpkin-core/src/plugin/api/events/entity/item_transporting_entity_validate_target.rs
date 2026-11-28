use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::EntityBase;

/// A copper golem checks whether a container is a valid target (Paper `ItemTransportingEntityValidateTargetEvent`).
#[derive(Event, Clone)]
pub struct ItemTransportingEntityValidateTargetEvent {
    pub entity: Arc<dyn EntityBase>,
    pub block: BlockPos,
    pub allowed: bool,
}
