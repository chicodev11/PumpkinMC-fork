use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player fails to get into a bed (Paper `PlayerBedFailEnterEvent`). Cancelling skips the failure message or explosion.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerBedFailEnterEvent {
    pub player: Arc<Player>,
    pub fail_reason: String,
    pub bed: BlockPos,
    pub will_explode: bool,
}

impl PlayerEvent for PlayerBedFailEnterEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
