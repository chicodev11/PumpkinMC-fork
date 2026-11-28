use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A chunk is sent to a player (Paper `PlayerChunkLoadEvent`).
#[derive(Event, Clone)]
pub struct PlayerChunkLoadEvent {
    pub player: Arc<Player>,
    pub chunk_x: i32,
    pub chunk_z: i32,
}

impl PlayerEvent for PlayerChunkLoadEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
