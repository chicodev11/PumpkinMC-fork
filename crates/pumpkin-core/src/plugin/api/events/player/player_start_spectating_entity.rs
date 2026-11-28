use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Paper `PlayerStartSpectatingEntityEvent`: a spectator starts viewing through an entity.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerStartSpectatingEntityEvent {
    pub player: Arc<Player>,
    pub current_spectator_target: Arc<dyn EntityBase>,
    pub new_spectator_target: Arc<dyn EntityBase>,
}

impl PlayerEvent for PlayerStartSpectatingEntityEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
