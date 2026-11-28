use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::world::World;

/// A world border moves its center (Paper `WorldBorderCenterChangeEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct WorldBorderCenterChangeEvent {
    pub world: Arc<World>,
    pub old_center_x: f64,
    pub old_center_z: f64,
    pub new_center_x: f64,
    pub new_center_z: f64,
}
