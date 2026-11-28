use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Paper `PlayerStopSpectatingEntityEvent`: a spectator stops viewing through an entity.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerStopSpectatingEntityEvent {
    pub player: Arc<Player>,
    pub spectator_target: Arc<dyn EntityBase>,
}

impl PlayerEvent for PlayerStopSpectatingEntityEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
