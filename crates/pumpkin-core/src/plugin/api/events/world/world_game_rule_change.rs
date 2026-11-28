use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::world::World;

/// Paper `WorldGameRuleChangeEvent`: a game rule is about to change. Change `value` or cancel.
#[cancellable]
#[derive(Event, Clone)]
pub struct WorldGameRuleChangeEvent {
    pub world: Arc<World>,
    pub game_rule: String,
    pub value: String,
}
