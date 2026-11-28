use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `EntityMoveEvent`: a living entity other than a player moved or turned during its
/// tick.
///
/// Cancelling puts it back at `from`; changing `to` moves it there instead, as Paper does.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityMoveEvent {
    pub entity: Arc<dyn EntityBase>,
    pub from: Vector3<f64>,
    pub from_yaw: f32,
    pub from_pitch: f32,
    pub to: Vector3<f64>,
    pub to_yaw: f32,
    pub to_pitch: f32,
}
