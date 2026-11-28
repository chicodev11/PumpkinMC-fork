use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A chunk is unloaded for a player (Paper `PlayerChunkUnloadEvent`).
#[derive(Event, Clone)]
pub struct PlayerChunkUnloadEvent {
    pub player: Arc<Player>,
    pub chunk_x: i32,
    pub chunk_z: i32,
}

impl PlayerEvent for PlayerChunkUnloadEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
