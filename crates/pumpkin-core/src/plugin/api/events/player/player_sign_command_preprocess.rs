use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player clicks a sign that runs a command (Paper `PlayerSignCommandPreprocessEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerSignCommandPreprocessEvent {
    pub player: Arc<Player>,
    pub message: String,
    pub sign: BlockPos,
}

impl PlayerEvent for PlayerSignCommandPreprocessEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
