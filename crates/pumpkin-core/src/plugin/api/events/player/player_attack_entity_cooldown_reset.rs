use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Paper `PlayerAttackEntityCooldownResetEvent`: an attack resets the attack charge. Cancel to keep it.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerAttackEntityCooldownResetEvent {
    pub player: Arc<Player>,
    pub attacked: Arc<dyn EntityBase>,
    pub cooldown: f32,
}

impl PlayerEvent for PlayerAttackEntityCooldownResetEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
