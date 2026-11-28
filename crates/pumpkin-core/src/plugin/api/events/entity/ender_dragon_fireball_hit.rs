use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A dragon fireball lands (Paper `EnderDragonFireballHitEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EnderDragonFireballHitEvent {
    pub fireball: Arc<dyn EntityBase>,
    pub targets: Vec<Arc<dyn EntityBase>>,
    pub area_effect_cloud: Arc<dyn EntityBase>,
}
