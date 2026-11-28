use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player's respawn point is set (Paper `PlayerSetSpawnEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerSetSpawnEvent {
    pub player: Arc<Player>,
    pub cause: String,
    pub location: Option<BlockPos>,
    pub forced: bool,
    pub notify_player: bool,
}

impl PlayerEvent for PlayerSetSpawnEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
