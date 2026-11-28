use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::player::Player;

/// Paper `BeaconEffectEvent`: a beacon gives a player its effect. Change the effect or cancel.
#[cancellable]
#[derive(Event, Clone)]
pub struct BeaconEffectEvent {
    pub block: BlockPos,
    pub player: Arc<Player>,
    pub effect_type: String,
    pub amplifier: i32,
    pub duration: i32,
    pub is_primary: bool,
}
