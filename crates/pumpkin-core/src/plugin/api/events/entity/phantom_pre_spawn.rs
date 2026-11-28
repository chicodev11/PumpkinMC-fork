use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use crate::entity::player::Player;

/// A phantom is about to spawn for a player (Paper `PhantomPreSpawnEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PhantomPreSpawnEvent {
    pub location: Vector3<f64>,
    pub entity: Arc<Player>,
    pub should_abort_spawn: bool,
}
