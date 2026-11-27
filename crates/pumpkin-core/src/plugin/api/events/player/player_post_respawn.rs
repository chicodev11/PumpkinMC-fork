use pumpkin_macros::Event;
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use super::PlayerEvent;
use crate::{entity::player::Player, world::World};

/// Paper `PlayerPostRespawnEvent`: fired once a respawn has finished and the player is in the
/// new world. Read-only.
#[derive(Event, Clone)]
pub struct PlayerPostRespawnEvent {
    pub player: Arc<Player>,
    pub world: Arc<World>,
    pub position: Vector3<f64>,
    pub yaw: f32,
    pub pitch: f32,
}

impl PlayerEvent for PlayerPostRespawnEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
