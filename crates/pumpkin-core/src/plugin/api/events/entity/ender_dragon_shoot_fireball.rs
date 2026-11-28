use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// The ender dragon shoots a fireball (Paper `EnderDragonShootFireballEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EnderDragonShootFireballEvent {
    pub dragon: Arc<dyn EntityBase>,
    pub fireball: Arc<dyn EntityBase>,
}
