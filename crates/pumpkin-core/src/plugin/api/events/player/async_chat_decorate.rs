use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A chat message is decorated (Paper `AsyncChatDecorateEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct AsyncChatDecorateEvent {
    pub player: Arc<Player>,
    pub original_message: String,
    pub result: String,
}

impl PlayerEvent for AsyncChatDecorateEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
