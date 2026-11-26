use std::sync::Arc;

use pumpkin_data::entity::EntityStatus;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::{BlockDirection, HorizontalFacingExt};
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use super::super::BrainTick;
use super::super::memory::position_tracker::{EntityTracker, PositionTracker};
use super::super::memory::{MemoryModuleId, MemoryStatus, types, walk_target::WalkTarget};
use super::timed::Behavior;
use crate::entity::EntityBase;
use crate::entity::ai::pathfinder::pathfinding_context::PathfindingContext;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::ai::util::goal_utils;
use crate::entity::mob::Mob;

const TIME_OUT_DURATION: i32 = 160;

const ENTRY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 4] = [
    (types::LOOK_TARGET.id(), MemoryStatus::Registered),
    (types::RAM_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
    (
        types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        MemoryStatus::ValuePresent,
    ),
    (types::RAM_TARGET.id(), MemoryStatus::ValueAbsent),
];

struct RamCandidate {
    start_position: BlockPos,
    target_position: BlockPos,
    target: Arc<dyn EntityBase>,
}

/// Vanilla `PrepareRamNearestTarget`: walks to a run-up spot in line with the nearest valid
/// target, waits there, then sets `RAM_TARGET` just past the target's block.
pub struct PrepareRamNearestTarget {
    get_cooldown_on_fail: fn(&dyn Mob) -> i32,
    min_ram_distance: i32,
    max_ram_distance: i32,
    walk_speed: f32,
    ram_targeting: TargetPredicate,
    ram_prepare_time: i64,
    get_prepare_ram_sound: fn(&dyn Mob) -> Sound,
    reached_ram_position_timestamp: Option<i64>,
    ram_candidate: Option<RamCandidate>,
}

impl PrepareRamNearestTarget {
    #[must_use]
    pub const fn new(
        get_cooldown_on_fail: fn(&dyn Mob) -> i32,
        min_ram_distance: i32,
        max_ram_distance: i32,
        walk_speed: f32,
        ram_targeting: TargetPredicate,
        ram_prepare_time: i64,
        get_prepare_ram_sound: fn(&dyn Mob) -> Sound,
    ) -> Self {
        Self {
            get_cooldown_on_fail,
            min_ram_distance,
            max_ram_distance,
            walk_speed,
            ram_targeting,
            ram_prepare_time,
            get_prepare_ram_sound,
            reached_ram_position_timestamp: None,
            ram_candidate: None,
        }
    }

    /// Vanilla `PrepareRamNearestTarget.isWalkableBlock`. Takes the navigation lock.
    fn is_walkable_block(mob: &dyn Mob, pos: &BlockPos) -> bool {
        let world = mob.get_entity().world.load_full();
        if goal_utils::is_not_stable(mob, &world, pos) {
            return false;
        }
        let mut context = PathfindingContext::new(pos.0, world);
        !goal_utils::has_malus(mob, &mut context, pos)
    }

    /// Vanilla `PrepareRamNearestTarget.calculateRammingStartPosition`.
    fn calculate_ramming_start_position(
        &self,
        mob: &dyn Mob,
        target: &dyn EntityBase,
    ) -> Option<BlockPos> {
        let target_pos = target.get_entity().block_pos.load();
        if !Self::is_walkable_block(mob, &target_pos) {
            return None;
        }
        let mut candidates = Vec::new();
        for direction in BlockDirection::horizontal_worldgen() {
            let offset = direction.to_block_direction().to_offset();
            let mut pos = target_pos;
            for _ in 0..self.max_ram_distance {
                let next = pos.offset(offset);
                if !Self::is_walkable_block(mob, &next) {
                    break;
                }
                pos = next;
            }
            if pos.manhattan_distance(target_pos) >= self.min_ram_distance {
                candidates.push(pos);
            }
        }
        let mob_pos = mob.get_entity().block_pos.load();
        candidates.sort_by_key(|pos| pos.squared_distance(&mob_pos));
        let steered = mob.get_mob_entity().steered();
        let owner = &steered.mob_entity().living_entity;
        let mut navigator = steered.navigator();
        candidates.into_iter().find(|pos| {
            navigator
                .create_path(owner, pos.0.to_f64(), 0)
                .is_some_and(|path| path.can_reach())
        })
    }

    /// Vanilla `PrepareRamNearestTarget.chooseRamPosition`.
    fn choose_ram_position(&mut self, mob: &dyn Mob, target: Arc<dyn EntityBase>) {
        self.reached_ram_position_timestamp = None;
        self.ram_candidate = self
            .calculate_ramming_start_position(mob, target.as_ref())
            .map(|start_position| RamCandidate {
                start_position,
                target_position: target.get_entity().block_pos.load(),
                target,
            });
    }

    /// Vanilla `PrepareRamNearestTarget.getEdgeOfBlock`: the target block's far side.
    fn get_edge_of_block(start: BlockPos, target: BlockPos) -> Vector3<f64> {
        let sign = |value: i32| f64::from(value.signum());
        let x_offset = 0.5 * sign(target.0.x - start.0.x);
        let z_offset = 0.5 * sign(target.0.z - start.0.z);
        target.to_f64() + Vector3::new(0.5 + x_offset, 0.0, 0.5 + z_offset)
    }
}

impl Behavior for PrepareRamNearestTarget {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        TIME_OUT_DURATION
    }

    fn max_duration(&self) -> i32 {
        TIME_OUT_DURATION
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let ctx = tick.visibility();
        let target = tick
            .state
            .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
            .and_then(|visible| {
                visible.find_closest(&ctx, |entity| {
                    self.ram_targeting
                        .test(tick.world, Some(tick.mob), entity.as_ref())
                })
            });
        if let Some(target) = target {
            self.choose_ram_position(tick.mob, target);
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if !tick.state.has_memory_value(types::RAM_TARGET.id()) {
            tick.world.send_entity_status(
                tick.mob.get_entity(),
                EntityStatus::EndRam,
                ActorEventID::None,
            );
            tick.state.set(
                types::RAM_COOLDOWN_TICKS,
                (self.get_cooldown_on_fail)(tick.mob),
            );
        }
    }

    fn can_still_use(&mut self, _tick: &mut BrainTick<'_>) -> bool {
        self.ram_candidate
            .as_ref()
            .is_some_and(|candidate| candidate.target.get_entity().is_alive())
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(candidate) = &self.ram_candidate else {
            return;
        };
        tick.state.set(
            types::WALK_TARGET,
            WalkTarget::from_block_pos(candidate.start_position, self.walk_speed, 0),
        );
        tick.state.set(
            types::LOOK_TARGET,
            Arc::new(EntityTracker::new(candidate.target.clone(), true))
                as Arc<dyn PositionTracker>,
        );
        let entity = tick.mob.get_entity();
        if candidate.target.get_entity().block_pos.load() != candidate.target_position {
            tick.world
                .send_entity_status(entity, EntityStatus::EndRam, ActorEventID::None);
            tick.mob.get_mob_entity().steered().navigator().stop();
            let target = candidate.target.clone();
            self.choose_ram_position(tick.mob, target);
        } else if entity.block_pos.load() == candidate.start_position {
            tick.world
                .send_entity_status(entity, EntityStatus::StartRam, ActorEventID::None);
            let reached = *self.reached_ram_position_timestamp.get_or_insert(tick.time);
            if tick.time - reached >= self.ram_prepare_time {
                tick.state.set(
                    types::RAM_TARGET,
                    Self::get_edge_of_block(candidate.start_position, candidate.target_position),
                );
                tick.world.play_sound_fine(
                    (self.get_prepare_ram_sound)(tick.mob),
                    SoundCategory::Neutral,
                    &entity.pos.load(),
                    1.0,
                    tick.mob.get_mob_entity().living_entity.get_pitch(),
                );
                self.ram_candidate = None;
            }
        }
    }

    fn debug_name(&self) -> &'static str {
        "PrepareRamNearestTarget"
    }
}
