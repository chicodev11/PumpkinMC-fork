use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::player::Player;

/// Paper `AnvilDamagedEvent`: using an anvil damages it. `damage-state` is FULL, CHIPPED, DAMAGED or BROKEN.
#[cancellable]
#[derive(Event, Clone)]
pub struct AnvilDamagedEvent {
    pub player: Arc<Player>,
    pub block: BlockPos,
    pub damage_state: String,
}
