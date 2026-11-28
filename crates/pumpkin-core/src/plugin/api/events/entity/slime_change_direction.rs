use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A slime picks a new direction (Paper `SlimeChangeDirectionEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct SlimeChangeDirectionEvent {
    pub slime: Arc<dyn EntityBase>,
    pub new_yaw: f32,
}
