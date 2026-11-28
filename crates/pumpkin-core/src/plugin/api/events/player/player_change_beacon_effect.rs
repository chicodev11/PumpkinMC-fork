use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player changes a beacon's effects (Paper `PlayerChangeBeaconEffectEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerChangeBeaconEffectEvent {
    pub player: Arc<Player>,
    pub beacon: BlockPos,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub consume_item: bool,
}

impl PlayerEvent for PlayerChangeBeaconEffectEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
