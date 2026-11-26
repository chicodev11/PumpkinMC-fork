//! Vanilla `Ghast.RandomFloatAroundGoal`.

use pumpkin_data::BlockDirection;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::chunk::ChunkHeightmapType;
use rand::RngExt;

use super::{Controls, Goal};
use crate::entity::mob::Mob;
use crate::world::World;

const MAX_ATTEMPTS: i32 = 64;

/// Picks a random spot within 16 blocks and floats there through the move control.
pub struct RandomFloatAroundGoal {
    distance_to_blocks: i32,
}

impl RandomFloatAroundGoal {
    /// `distance_to_blocks` > 0 prefers spots within that many blocks of something solid.
    #[must_use]
    pub const fn new(distance_to_blocks: i32) -> Self {
        Self { distance_to_blocks }
    }
}

fn choose_random_position(pos: Vector3<f64>, mob: &dyn Mob) -> Vector3<f64> {
    let mut rng = mob.get_random();
    let mut axis = || f64::from(rng.random::<f32>().mul_add(2.0, -1.0) * 16.0);
    Vector3::new(pos.x + axis(), pos.y + axis(), pos.z + axis())
}

fn is_good_target(world: &World, pos: Vector3<f64>, distance_to_blocks: i32) -> bool {
    if distance_to_blocks <= 0 {
        return true;
    }
    let block_pos = BlockPos::floored_v(pos);
    if !world.get_block_state(&block_pos).is_air() {
        return false;
    }
    BlockDirection::all().iter().any(|direction| {
        (1..distance_to_blocks).any(|i| {
            let offset = direction.to_offset();
            let relative = block_pos.offset(Vector3::new(offset.x * i, offset.y * i, offset.z * i));
            !world.get_block_state(&relative).is_air()
        })
    })
}

/// Vanilla `getSuitableFlyToPosition`.
#[must_use]
pub fn get_suitable_fly_to_position(mob: &dyn Mob, distance_to_blocks: i32) -> Vector3<f64> {
    let entity = mob.get_entity();
    let world = entity.world.load();
    let pos = entity.pos.load();
    for _ in 0..MAX_ATTEMPTS {
        let candidate = choose_random_position(pos, mob);
        let within_home = mob
            .get_mob_entity()
            .is_in_position_target_range_pos(&BlockPos::floored_v(candidate));
        if within_home && is_good_target(&world, candidate, distance_to_blocks) {
            return candidate;
        }
    }
    let mut result = choose_random_position(pos, mob);
    let block_pos = BlockPos::floored_v(result);
    let height = world.get_heightmap_height(
        ChunkHeightmapType::MotionBlocking,
        block_pos.0.x,
        block_pos.0.z,
    );
    if height < block_pos.0.y && height > world.min_y {
        result.y = pos.y - (pos.y - result.y).abs();
    }
    result
}

impl Goal for RandomFloatAroundGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let move_control = mob
            .get_mob_entity()
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !move_control.has_wanted() {
            return true;
        }
        let distance = move_control
            .get_wanted_position()
            .squared_distance_to_vec(&mob.get_entity().pos.load());
        !(1.0..=3600.0).contains(&distance)
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn Mob) {
        let target = get_suitable_fly_to_position(mob, self.distance_to_blocks);
        mob.get_mob_entity()
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_wanted_position(target.x, target.y, target.z, 1.0);
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}
