use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Paper `PlayerLaunchProjectileEvent`: a player throws a projectile item (snowball, egg, ender pearl,
/// experience bottle, splash/lingering potion, wind charge, trident). Cancel to stop the throw.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerLaunchProjectileEvent {
    pub player: Arc<Player>,
    pub item: ItemStack,
    pub projectile: Arc<dyn EntityBase>,
    pub should_consume: bool,
}

impl PlayerEvent for PlayerLaunchProjectileEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
