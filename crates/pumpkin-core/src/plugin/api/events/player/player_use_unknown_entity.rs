use pumpkin_macros::Event;
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player interacts with an entity the server does not know (Paper `PlayerUseUnknownEntityEvent`).
#[derive(Event, Clone)]
pub struct PlayerUseUnknownEntityEvent {
    pub player: Arc<Player>,
    pub entity_id: i32,
    pub attack: bool,
    pub hand: String,
    pub clicked_position: Option<Vector3<f64>>,
}

impl PlayerEvent for PlayerUseUnknownEntityEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
