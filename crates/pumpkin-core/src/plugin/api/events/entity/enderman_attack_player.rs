use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Paper `EndermanAttackPlayerEvent`: an enderman checks whether a player stares at it. Change `should-attack`.
#[derive(Event, Clone)]
pub struct EndermanAttackPlayerEvent {
    pub enderman: Arc<dyn EntityBase>,
    pub player: Arc<Player>,
    pub should_attack: bool,
}
