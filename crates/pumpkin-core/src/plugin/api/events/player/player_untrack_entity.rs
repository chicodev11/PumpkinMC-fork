use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::{EntityBase, player::Player};

/// Paper `PlayerUntrackEntityEvent`: a player stops tracking an entity, because it left the
/// tracking range or was removed. Read-only.
#[derive(Event, Clone)]
pub struct PlayerUntrackEntityEvent {
    pub player: Arc<Player>,
    pub entity: Arc<dyn EntityBase>,
}

impl PlayerEvent for PlayerUntrackEntityEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
