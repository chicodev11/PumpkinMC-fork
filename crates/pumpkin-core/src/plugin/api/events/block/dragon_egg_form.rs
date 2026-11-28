use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

/// Paper `DragonEggFormEvent`: the dragon egg appears after the first dragon kill.
#[cancellable]
#[derive(Event, Clone)]
pub struct DragonEggFormEvent {
    pub block: BlockPos,
}
