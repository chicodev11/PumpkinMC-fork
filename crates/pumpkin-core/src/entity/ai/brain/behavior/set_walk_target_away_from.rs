use std::sync::Arc;

use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use super::super::BrainTick;
use super::super::memory::{
    MemoryModuleId, MemoryModuleType, MemoryStatus, types, walk_target::WalkTarget,
};
use super::OneShot;
use crate::entity::EntityBase;
use crate::entity::ai::util::land_random_pos;

/// Vanilla `SetWalkTargetAwayFrom.pos`: walk away from a remembered block.
#[must_use]
pub fn pos(
    memory: MemoryModuleType<BlockPos>,
    speed_modifier: f32,
    desired_distance: i32,
    override_walk_target: bool,
) -> OneShot {
    create(
        memory.id(),
        speed_modifier,
        desired_distance,
        override_walk_target,
        move |tick| tick.state.get(memory).map(BlockPos::to_centered_f64),
    )
}

/// Vanilla `SetWalkTargetAwayFrom.entity`: walk away from a remembered entity.
#[must_use]
pub fn entity(
    memory: MemoryModuleType<Arc<dyn EntityBase>>,
    speed_modifier: f32,
    desired_distance: i32,
    override_walk_target: bool,
) -> OneShot {
    create(
        memory.id(),
        speed_modifier,
        desired_distance,
        override_walk_target,
        move |tick| {
            tick.state
                .get(memory)
                .map(|entity| entity.get_entity().pos.load())
        },
    )
}

fn create(
    memory: MemoryModuleId,
    speed_modifier: f32,
    desired_distance: i32,
    override_walk_target: bool,
    avoid_position: impl Fn(&BrainTick<'_>) -> Option<Vector3<f64>> + Send + Sync + 'static,
) -> OneShot {
    OneShot::new(
        "SetWalkTargetAwayFrom",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (memory, MemoryStatus::ValuePresent),
        ],
        move |tick| {
            let walking = tick
                .state
                .get(types::WALK_TARGET)
                .map(|walk| (walk.speed_modifier(), walk.target().current_position()));
            if walking.is_some() && !override_walk_target {
                return false;
            }
            let Some(avoid) = avoid_position(tick) else {
                return false;
            };
            let pos = tick.mob.get_entity().pos.load();
            let desired = f64::from(desired_distance);
            if pos.squared_distance_to_vec(&avoid) >= desired * desired {
                return false;
            }
            // Already heading away at this speed: keep going.
            if let Some((speed, walk_pos)) = walking
                && speed == speed_modifier
                && (walk_pos - pos).dot(&(avoid - pos)) < 0.0
            {
                return false;
            }
            for _ in 0..10 {
                if let Some(target) = land_random_pos::get_pos_away(tick.mob, 16, 7, avoid) {
                    tick.state.set(
                        types::WALK_TARGET,
                        WalkTarget::from_vec(target, speed_modifier, 0),
                    );
                    break;
                }
            }
            true
        },
    )
}
