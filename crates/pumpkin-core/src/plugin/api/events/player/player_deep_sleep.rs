use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerDeepSleepEvent`: the player has slept long enough to count for skipping the night.
/// Cancelling restarts the sleep count.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerDeepSleepEvent {
    pub player: Arc<Player>,
}

impl PlayerEvent for PlayerDeepSleepEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
