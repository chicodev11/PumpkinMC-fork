use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// The command tree is sent to a player (Paper `AsyncPlayerSendCommandsEvent`). Removing a root command hides it from the client.
#[derive(Event, Clone)]
pub struct AsyncPlayerSendCommandsEvent {
    pub player: Arc<Player>,
    pub commands: Vec<String>,
}

impl PlayerEvent for AsyncPlayerSendCommandsEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
