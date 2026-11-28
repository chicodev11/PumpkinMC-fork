use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `ClientTickEndEvent`: the client finished a tick. Keep handlers quick.
#[derive(Event, Clone)]
pub struct ClientTickEndEvent {
    pub player: Arc<Player>,
}

impl PlayerEvent for ClientTickEndEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
