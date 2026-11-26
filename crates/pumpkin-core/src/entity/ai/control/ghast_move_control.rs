//! Vanilla `Ghast.GhastMoveControl`: a floating mob drifts straight at its wanted position,
//! nudging its velocity every few ticks while the way there stays clear.

use pumpkin_data::attributes::Attributes;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::ai::control::move_control::Operation;
use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::mob::Mob;

pub struct GhastMoveControl {
    wanted: Vector3<f64>,
    speed_modifier: f64,
    operation: Operation,
    float_duration: i32,
    /// Happy ghasts keep clear of fluids and the `happy_ghast_avoids` blocks.
    careful: bool,
}

impl GhastMoveControl {
    #[must_use]
    pub const fn new(careful: bool) -> Self {
        Self {
            wanted: Vector3::new(0.0, 0.0, 0.0),
            speed_modifier: 0.0,
            operation: Operation::Wait,
            float_duration: 0,
            careful,
        }
    }

    /// Vanilla `canReach`.
    fn can_reach(&self, mob: &dyn Mob, delta: Vector3<f64>) -> bool {
        let entity = mob.get_entity();
        let world = entity.world.load();
        let bounding_box = entity.bounding_box.load();
        if self.careful {
            let moved = bounding_box.shift(delta).expand_all(1.0);
            return BlockPos::iterate(moved.min_block_pos(), moved.max_block_pos())
                .all(|pos| careful_traversal_possible(&world, &pos));
        }
        // Vanilla `forEachBlockIntersectedBetween` + `blockTraversalPossible(from, to)`: every
        // block the box passes through, except those it already overlaps, must leave the swept
        // box clear.
        let from = entity.pos.load();
        pumpkin_util::math::block_traversal::for_each_block_intersected_between(
            from,
            from + delta,
            &bounding_box.shift(delta),
            |pos, _| {
                if bounding_box.intersects(&BoundingBox::from_block(&pos)) {
                    return true;
                }
                let state = world.get_block_state(&pos);
                state.is_air()
                    || !pumpkin_util::math::block_traversal::collided_along_vector(
                        &bounding_box,
                        delta,
                        state.get_block_collision_shapes_at(&pos),
                    )
            },
        )
    }
}

/// Vanilla `blockTraversalPossible` for a careful mob with no movement path.
fn careful_traversal_possible(world: &crate::world::World, pos: &BlockPos) -> bool {
    let state = world.get_block_state(pos);
    if state.is_air() {
        return true;
    }
    if state.get_block_collision_shapes_at(pos).next().is_some() {
        return false;
    }
    if world
        .get_block(pos)
        .has_tag(&tag::Block::MINECRAFT_HAPPY_GHAST_AVOIDS)
    {
        return false;
    }
    let fluid = world.get_fluid(pos);
    !(fluid.id == Fluid::WATER.id
        || fluid.id == Fluid::FLOWING_WATER.id
        || fluid.id == Fluid::LAVA.id
        || fluid.id == Fluid::FLOWING_LAVA.id)
}

/// Vanilla `Mob.stopInPlace`.
pub fn stop_in_place(mob: &dyn Mob) {
    let mob_entity = mob.get_mob_entity();
    mob_entity.steered().navigator().stop();
    let living = &mob_entity.living_entity;
    living.movement_input.store(Vector3::new(0.0, 0.0, 0.0));
    mob_entity.set_speed(0.0);
    living.entity.velocity.store(Vector3::new(0.0, 0.0, 0.0));
}

impl Control for GhastMoveControl {}

impl MoveControlTrait for GhastMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        if mob.ghast_should_stop() {
            self.operation = Operation::Wait;
            stop_in_place(mob);
        }
        if self.operation != Operation::MoveTo {
            return;
        }
        let previous = self.float_duration;
        self.float_duration -= 1;
        if previous > 0 {
            return;
        }
        self.float_duration += mob.get_random().random_range(0..5) + 2;
        let entity = mob.get_entity();
        let delta = self.wanted - entity.pos.load();
        if self.can_reach(mob, delta) {
            let speed = mob
                .get_mob_entity()
                .living_entity
                .get_attribute_value(&Attributes::FLYING_SPEED)
                * 5.0
                / 3.0;
            let length = delta.length();
            let direction = if length < 1.0e-5 {
                Vector3::new(0.0, 0.0, 0.0)
            } else {
                delta * (1.0 / length)
            };
            entity
                .velocity
                .store(entity.velocity.load() + direction * speed);
        } else {
            self.operation = Operation::Wait;
        }
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.wanted = Vector3::new(x, y, z);
        self.speed_modifier = speed_modifier;
        self.operation = Operation::MoveTo;
    }

    fn set_wait(&mut self) {
        self.operation = Operation::Wait;
    }

    fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }

    fn get_speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    fn get_wanted_position(&self) -> Vector3<f64> {
        self.wanted
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_util::math::block_traversal::collided_along_vector;
    use pumpkin_util::math::boundingbox::BoundingBox;
    use pumpkin_util::math::vector3::Vector3;

    fn cube(min: (f64, f64, f64), max: (f64, f64, f64)) -> BoundingBox {
        BoundingBox::new(
            Vector3::new(min.0, min.1, min.2),
            Vector3::new(max.0, max.1, max.2),
        )
    }

    #[test]
    fn swept_box_hits_thin_walls_and_misses_clear_paths() {
        let ghast = cube((0.0, 0.0, 0.0), (4.0, 4.0, 4.0));
        let east = Vector3::new(6.0, 0.0, 0.0);
        // A pane-thin wall between the old one-block steps is still in the way.
        let thin_wall = cube((7.45, 0.0, 0.0), (7.55, 4.0, 4.0));
        assert!(collided_along_vector(&ghast, east, [thin_wall].into_iter()));
        // Beside the path, above it, or behind the start: clear.
        let beside = cube((5.0, 0.0, 4.5), (6.0, 4.0, 5.5));
        let above = cube((5.0, 4.5, 0.0), (6.0, 5.5, 4.0));
        let behind = cube((-2.0, 0.0, 0.0), (-1.0, 4.0, 4.0));
        assert!(!pumpkin_util::math::block_traversal::collided_along_vector(
            &ghast,
            east,
            [beside, above, behind].into_iter()
        ));
        // Touching faces do not count: the box is shrunk by 1e-7.
        let touching = cube((10.0, 0.0, 0.0), (11.0, 4.0, 4.0));
        assert!(!pumpkin_util::math::block_traversal::collided_along_vector(
            &ghast,
            east,
            [touching].into_iter()
        ));
    }
}
