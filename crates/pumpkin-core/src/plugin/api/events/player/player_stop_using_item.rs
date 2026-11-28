use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerStopUsingItemEvent`: a player let go of an item they were using (a bow,
/// a shield, food) before it finished. Read-only.
#[derive(Event, Clone)]
pub struct PlayerStopUsingItemEvent {
    pub player: Arc<Player>,
    pub item: ItemStack,
    pub ticks_held_for: i32,
}

impl PlayerEvent for PlayerStopUsingItemEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
