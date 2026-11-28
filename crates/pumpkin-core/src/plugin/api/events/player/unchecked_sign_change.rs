use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player edits a sign that is not in the world (Paper `UncheckedSignChangeEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct UncheckedSignChangeEvent {
    pub player: Arc<Player>,
    pub location: BlockPos,
    pub side: String,
    pub lines: Vec<String>,
}

impl PlayerEvent for UncheckedSignChangeEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
