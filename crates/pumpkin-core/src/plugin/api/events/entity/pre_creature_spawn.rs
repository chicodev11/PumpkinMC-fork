use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;

/// A mob is about to spawn naturally (Paper `PreCreatureSpawnEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PreCreatureSpawnEvent {
    pub location: Vector3<f64>,
    pub entity_type: String,
    pub reason: String,
    pub should_abort_spawn: bool,
}
