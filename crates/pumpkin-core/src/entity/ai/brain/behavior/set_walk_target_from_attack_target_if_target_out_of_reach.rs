use std::sync::Arc;

use super::super::memory::{
    MemoryStatus, position_tracker::EntityTracker, types, walk_target::WalkTarget,
};
use super::OneShot;
use super::utils::is_within_attack_range;
use crate::entity::mob::Mob;

/// Vanilla `SetWalkTargetFromAttackTargetIfTargetOutOfReach`: walk at the attack target until
/// it is visible and within reach.
#[must_use]
pub fn create(speed_modifier: f32) -> OneShot {
    create_with(move |_| speed_modifier)
}

/// [`create`] with a speed that depends on the mob, as vanilla's function overload has it.
#[must_use]
pub fn create_with(speed_modifier: impl Fn(&dyn Mob) -> f32 + Send + Sync + 'static) -> OneShot {
    OneShot::new(
        "SetWalkTargetFromAttackTargetIfTargetOutOfReach",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::Registered,
            ),
        ],
        move |tick| {
            let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
                return false;
            };
            let visible = tick
                .state
                .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
                .is_some_and(|visible| visible.contains(target.as_ref(), &tick.visibility()));
            if visible && is_within_attack_range(tick.mob, target.as_ref(), 1) {
                tick.state.erase(types::WALK_TARGET.id());
            } else {
                tick.state.set(
                    types::LOOK_TARGET,
                    Arc::new(EntityTracker::new(target.clone(), true)) as _,
                );
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::new(
                        Arc::new(EntityTracker::new(target, false)),
                        speed_modifier(tick.mob),
                        0,
                    ),
                );
            }
            true
        },
    )
}
