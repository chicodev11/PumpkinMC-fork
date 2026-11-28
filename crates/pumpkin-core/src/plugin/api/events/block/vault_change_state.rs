use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

/// Paper `VaultChangeStateEvent`: a vault changes state (INACTIVE, ACTIVE, UNLOCKING, EJECTING).
#[cancellable]
#[derive(Event, Clone)]
pub struct VaultChangeStateEvent {
    pub block: BlockPos,
    pub current_state: String,
    pub new_state: String,
}
