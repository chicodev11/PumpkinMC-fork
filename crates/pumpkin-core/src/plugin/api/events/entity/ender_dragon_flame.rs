use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// The ender dragon breathes flame (Paper `EnderDragonFlameEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EnderDragonFlameEvent {
    pub dragon: Arc<dyn EntityBase>,
    pub area_effect_cloud: Arc<dyn EntityBase>,
}
