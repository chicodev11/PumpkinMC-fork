use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerItemGroupCooldownEvent`: a cooldown group is about to start. Change `cooldown` (ticks) or cancel.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerItemGroupCooldownEvent {
    pub player: Arc<Player>,
    pub cooldown_group: String,
    pub cooldown: i32,
}

impl PlayerEvent for PlayerItemGroupCooldownEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
