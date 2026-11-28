use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerItemCooldownEvent`: an item's use starts a cooldown. Change `cooldown` (ticks) or cancel.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerItemCooldownEvent {
    pub player: Arc<Player>,
    pub item_type: String,
    pub cooldown_group: String,
    pub cooldown: i32,
}

impl PlayerEvent for PlayerItemCooldownEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
