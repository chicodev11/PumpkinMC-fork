use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::{EntityBase, player::Player};

/// Paper `PlayerPickupExperienceEvent`: a player is about to pick up an experience orb.
///
/// Fired before the pickup cooldown starts; cancelling leaves the orb and the cooldown alone.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerPickupExperienceEvent {
    pub player: Arc<Player>,
    pub orb: Arc<dyn EntityBase>,
    /// The experience one pickup of this orb gives. Read-only.
    pub experience: i32,
}

impl PlayerEvent for PlayerPickupExperienceEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
