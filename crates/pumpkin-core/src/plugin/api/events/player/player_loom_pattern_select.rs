use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player selects a banner pattern in a loom (Paper `PlayerLoomPatternSelectEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerLoomPatternSelectEvent {
    pub player: Arc<Player>,
    pub pattern: String,
}

impl PlayerEvent for PlayerLoomPatternSelectEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
