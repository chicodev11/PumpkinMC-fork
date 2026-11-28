use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use crate::entity::EntityBase;

/// An entity is knocked back by an attack (Paper `EntityPushedByEntityAttackEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityPushedByEntityAttackEvent {
    pub entity: Arc<dyn EntityBase>,
    pub pushed_by: Arc<dyn EntityBase>,
    pub cause: String,
    pub knockback: Vector3<f64>,
}
