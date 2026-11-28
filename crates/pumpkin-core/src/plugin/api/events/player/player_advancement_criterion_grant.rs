use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player is granted an advancement criterion (Paper `PlayerAdvancementCriterionGrantEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerAdvancementCriterionGrantEvent {
    pub player: Arc<Player>,
    pub advancement: String,
    pub criterion: String,
}

impl PlayerEvent for PlayerAdvancementCriterionGrantEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
