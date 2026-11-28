use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player selects a stonecutter recipe (Paper `PlayerStonecutterRecipeSelectEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerStonecutterRecipeSelectEvent {
    pub player: Arc<Player>,
    pub recipe: String,
}

impl PlayerEvent for PlayerStonecutterRecipeSelectEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
