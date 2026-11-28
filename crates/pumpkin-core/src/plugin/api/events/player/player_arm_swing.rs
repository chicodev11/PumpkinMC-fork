use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerArmSwingEvent`: the player swings a hand (`HAND` or `OFF_HAND`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerArmSwingEvent {
    pub player: Arc<Player>,
    pub hand: String,
}

impl PlayerEvent for PlayerArmSwingEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
