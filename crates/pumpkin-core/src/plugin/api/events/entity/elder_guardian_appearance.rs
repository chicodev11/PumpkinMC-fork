use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Paper `ElderGuardianAppearanceEvent`: an elder guardian curses a player (the face and mining fatigue).
#[cancellable]
#[derive(Event, Clone)]
pub struct ElderGuardianAppearanceEvent {
    pub guardian: Arc<dyn EntityBase>,
    pub affected_player: Arc<Player>,
}
