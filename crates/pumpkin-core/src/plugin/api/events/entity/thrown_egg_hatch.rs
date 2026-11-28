use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `ThrownEggHatchEvent`: a thrown egg broke. Change `hatching` or `num-hatches`.
#[derive(Event, Clone)]
pub struct ThrownEggHatchEvent {
    pub egg: Arc<dyn EntityBase>,
    pub hatching: bool,
    pub num_hatches: i32,
    pub hatching_type: String,
}
