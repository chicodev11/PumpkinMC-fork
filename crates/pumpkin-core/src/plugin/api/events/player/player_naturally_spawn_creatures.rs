use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Mobs may spawn around a player (Paper `PlayerNaturallySpawnCreaturesEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerNaturallySpawnCreaturesEvent {
    pub player: Arc<Player>,
    pub radius: i32,
}

impl PlayerEvent for PlayerNaturallySpawnCreaturesEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
