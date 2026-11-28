use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Command suggestions are sent to a player (Paper `AsyncPlayerSendSuggestionsEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct AsyncPlayerSendSuggestionsEvent {
    pub player: Arc<Player>,
    pub buffer: String,
    pub suggestions: Vec<String>,
}

impl PlayerEvent for AsyncPlayerSendSuggestionsEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
