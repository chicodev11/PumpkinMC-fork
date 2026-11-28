use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// Paper `ExperienceOrbMergeEvent`: one experience orb absorbs another.
#[cancellable]
#[derive(Event, Clone)]
pub struct ExperienceOrbMergeEvent {
    pub merge_target: Arc<dyn EntityBase>,
    pub merge_source: Arc<dyn EntityBase>,
}
