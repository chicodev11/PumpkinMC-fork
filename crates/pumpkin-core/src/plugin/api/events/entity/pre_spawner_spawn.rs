use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

/// A spawner is about to spawn an entity (Paper `PreSpawnerSpawnEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PreSpawnerSpawnEvent {
    pub location: Vector3<f64>,
    pub entity_type: String,
    pub spawner_location: BlockPos,
    pub should_abort_spawn: bool,
}
