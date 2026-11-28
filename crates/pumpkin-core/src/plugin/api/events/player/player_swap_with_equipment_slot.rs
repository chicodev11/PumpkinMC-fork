use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerSwapWithEquipmentSlotEvent`: right-clicking equips the held item (armor, elytra, heads).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerSwapWithEquipmentSlotEvent {
    pub player: Arc<Player>,
    pub hand: String,
    pub slot: String,
    pub item: ItemStack,
}

impl PlayerEvent for PlayerSwapWithEquipmentSlotEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
