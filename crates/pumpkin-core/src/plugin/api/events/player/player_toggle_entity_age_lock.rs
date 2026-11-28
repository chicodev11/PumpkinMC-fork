use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::EntityBase;
use crate::entity::player::Player;

/// A player locks or unlocks the age of an entity (Paper `PlayerToggleEntityAgeLockEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerToggleEntityAgeLockEvent {
    pub player: Arc<Player>,
    pub entity: Arc<dyn EntityBase>,
    pub item: ItemStack,
    pub age_locked: bool,
}

impl PlayerEvent for PlayerToggleEntityAgeLockEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
