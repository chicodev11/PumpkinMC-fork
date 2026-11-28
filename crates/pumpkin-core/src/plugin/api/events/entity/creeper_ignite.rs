use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `CreeperIgniteEvent`: a creeper is lit with flint and steel.
#[cancellable]
#[derive(Event, Clone)]
pub struct CreeperIgniteEvent {
    pub creeper: Arc<dyn EntityBase>,
    pub ignited: bool,
}
