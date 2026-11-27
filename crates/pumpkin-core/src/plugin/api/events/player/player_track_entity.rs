use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::{EntityBase, player::Player};

/// Paper `PlayerTrackEntityEvent`: an entity comes into a player's tracking range and is about
/// to be spawned on the client.
///
/// Cancelling skips the spawn packets; like Paper, the player still counts as tracking it.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerTrackEntityEvent {
    pub player: Arc<Player>,
    pub entity: Arc<dyn EntityBase>,
}

impl PlayerEvent for PlayerTrackEntityEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
