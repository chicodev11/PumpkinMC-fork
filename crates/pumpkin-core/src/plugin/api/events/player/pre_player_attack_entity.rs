use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::{EntityBase, player::Player};

/// Paper `PrePlayerAttackEntityEvent`: a player is about to attack an entity.
///
/// `will_attack` is vanilla's `isAttackable() && !skipAttackInteraction(player)`. Cancelling
/// stops the attack.
#[cancellable]
#[derive(Event, Clone)]
pub struct PrePlayerAttackEntityEvent {
    pub player: Arc<Player>,
    pub attacked: Arc<dyn EntityBase>,
    pub will_attack: bool,
}

impl PlayerEvent for PrePlayerAttackEntityEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
