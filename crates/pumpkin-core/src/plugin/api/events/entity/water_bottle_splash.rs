use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A splash water bottle lands (Paper `WaterBottleSplashEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct WaterBottleSplashEvent {
    pub potion: Arc<dyn EntityBase>,
    pub to_damage: Vec<Arc<dyn EntityBase>>,
}
