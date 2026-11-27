use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerJumpEvent`: the client reported a jump (left the ground moving up).
///
/// Cancelling sends the player back to `from`, as Paper does.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerJumpEvent {
    pub player: Arc<Player>,
    pub from: Vector3<f64>,
    pub to: Vector3<f64>,
}

impl PlayerJumpEvent {
    #[must_use]
    pub const fn new(player: Arc<Player>, from: Vector3<f64>, to: Vector3<f64>) -> Self {
        Self {
            player,
            from,
            to,
            cancelled: false,
        }
    }
}

impl PlayerEvent for PlayerJumpEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
