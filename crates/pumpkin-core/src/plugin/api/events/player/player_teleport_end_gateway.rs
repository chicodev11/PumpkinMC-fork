use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player is teleported by an end gateway (Paper `PlayerTeleportEndGatewayEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerTeleportEndGatewayEvent {
    pub player: Arc<Player>,
    pub from: Vector3<f64>,
    pub to: Vector3<f64>,
    pub gateway: BlockPos,
}

impl PlayerEvent for PlayerTeleportEndGatewayEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
