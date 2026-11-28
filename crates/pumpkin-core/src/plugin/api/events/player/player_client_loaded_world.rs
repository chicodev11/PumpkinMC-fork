use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player's client finished loading the world, or the wait for it ran out (Paper `PlayerClientLoadedWorldEvent`).
#[derive(Event, Clone)]
pub struct PlayerClientLoadedWorldEvent {
    pub player: Arc<Player>,
    pub timeout: bool,
}

impl PlayerEvent for PlayerClientLoadedWorldEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
