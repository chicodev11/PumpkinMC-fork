use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Paper `PlayerShieldDisableEvent`: an attack disables the player's shield. Change `cooldown` or cancel.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerShieldDisableEvent {
    pub player: Arc<Player>,
    pub damager: Option<Arc<dyn EntityBase>>,
    pub cooldown: i32,
}

impl PlayerEvent for PlayerShieldDisableEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
