use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use crate::entity::EntityBase;

/// An entity is teleported by an end gateway (Paper `EntityTeleportEndGatewayEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityTeleportEndGatewayEvent {
    pub entity: Arc<dyn EntityBase>,
    pub from: Vector3<f64>,
    pub to: Vector3<f64>,
    pub gateway: BlockPos,
}
