use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::EntityBase;
use crate::entity::player::Player;

/// A player attempts a mace smash attack (Paper `EntityAttemptSmashAttackEvent`). `result` is `DEFAULT`, `ALLOW` or `DENY`.
#[derive(Event, Clone)]
pub struct EntityAttemptSmashAttackEvent {
    pub entity: Arc<Player>,
    pub target: Arc<dyn EntityBase>,
    pub weapon: ItemStack,
    pub original_result: bool,
    pub result: String,
}
