use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// A player touches an item they could pick up (Paper `PlayerAttemptPickupItemEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerAttemptPickupItemEvent {
    pub player: Arc<Player>,
    pub item_entity_id: i32,
    pub item: ItemStack,
}

impl PlayerEvent for PlayerAttemptPickupItemEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
