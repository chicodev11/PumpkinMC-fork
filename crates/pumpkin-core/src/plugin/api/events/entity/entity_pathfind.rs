use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `EntityPathfindEvent`: a mob starts finding a path to `location`.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityPathfindEvent {
    pub entity: Arc<dyn EntityBase>,
    pub location: Vector3<f64>,
    pub target_entity: Option<Arc<dyn EntityBase>>,
}
