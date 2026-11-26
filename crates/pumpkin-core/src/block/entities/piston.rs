use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::block_properties::{Axis, PistonHeadLikeProperties, PistonType};
use pumpkin_data::entity::EntityType;
use pumpkin_data::{Block, BlockDirection, BlockState};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3};

use crate::entity::EntityBase;
use crate::world::{BlockFlags, World};
use pumpkin_data::block_properties::is_air;

use super::BlockEntity;

type PistonProps = pumpkin_data::block_properties::StickyPistonLikeProperties;

thread_local! {
    /// Vanilla `PistonMovingBlockEntity.NOCLIP`: the direction pistons are moving an entity
    /// right now, so blocks moving that way do not stop it.
    static NOCLIP: std::cell::Cell<Option<BlockDirection>> = const { std::cell::Cell::new(None) };
}

pub struct PistonBlockEntity {
    pub position: BlockPos,
    pub pushed_block_state: &'static BlockState,
    pub facing: BlockDirection,
    pub current_progress: AtomicCell<f32>,
    pub last_progress: AtomicCell<f32>,
    pub extending: bool,
    pub source: bool,
    /// Vanilla `lastTicked`: the game time of the last tick.
    pub last_ticked: AtomicI64,
}

impl PistonBlockEntity {
    pub const ID: &'static str = "minecraft:piston";

    const fn movement_direction(&self) -> BlockDirection {
        if self.extending {
            self.facing
        } else {
            self.facing.opposite()
        }
    }

    /// Vanilla's `getAmountExtended`: how far back from the block's final position
    /// the visual is at a given animation progress. Negative for extending.
    fn amount_extended(&self, progress: f32) -> f32 {
        if self.extending {
            progress - 1.0
        } else {
            1.0 - progress
        }
    }

    fn dir_vec(dir: BlockDirection, scale: f64) -> Vector3<f64> {
        let off = dir.to_offset();
        Vector3::new(
            f64::from(off.x) * scale,
            f64::from(off.y) * scale,
            f64::from(off.z) * scale,
        )
    }

    /// Vanilla `getCollisionRelatedBlockState`: a retracting piston pushes with its head.
    fn collision_related_state(&self) -> &'static BlockState {
        let block = Block::from_state_id(self.pushed_block_state.id);
        if !self.extending
            && self.source
            && (block == &Block::PISTON || block == &Block::STICKY_PISTON)
        {
            let base = PistonProps::from_state_id(self.pushed_block_state.id);
            let mut head = PistonHeadLikeProperties::default(&Block::PISTON_HEAD);
            head.short = self.current_progress.load() > 0.25;
            head.r#type = if block == &Block::STICKY_PISTON {
                PistonType::Sticky
            } else {
                PistonType::Normal
            };
            head.facing = base.facing;
            return BlockState::from_id(head.to_state_id(&Block::PISTON_HEAD));
        }
        self.pushed_block_state
    }

    /// Vanilla `moveByPositionAndProgress`: a block-local box where the moving block is now.
    fn at_progress(&self, local: BoundingBox) -> BoundingBox {
        local.at_pos(self.position).shift(Self::dir_vec(
            self.facing,
            f64::from(self.amount_extended(self.current_progress.load())),
        ))
    }

    /// Vanilla `PistonMath.getMovementArea`: the slab the moving face sweeps this tick.
    fn movement_area(aabb: BoundingBox, dir: BlockDirection, delta: f64) -> BoundingBox {
        let (min, max) = (aabb.min, aabb.max);
        let (lo, hi) = match dir {
            BlockDirection::West => ((min.x - delta, min.y, min.z), (min.x, max.y, max.z)),
            BlockDirection::East => ((max.x, min.y, min.z), (max.x + delta, max.y, max.z)),
            BlockDirection::Down => ((min.x, min.y - delta, min.z), (max.x, min.y, max.z)),
            BlockDirection::Up => ((min.x, max.y, min.z), (max.x, max.y + delta, max.z)),
            BlockDirection::North => ((min.x, min.y, min.z - delta), (max.x, max.y, min.z)),
            BlockDirection::South => ((min.x, min.y, max.z), (max.x, max.y, max.z + delta)),
        };
        BoundingBox::new(
            Vector3::new(lo.0, lo.1, lo.2),
            Vector3::new(hi.0, hi.1, hi.2),
        )
    }

    const fn union(a: BoundingBox, b: BoundingBox) -> BoundingBox {
        BoundingBox::new(
            Vector3::new(
                a.min.x.min(b.min.x),
                a.min.y.min(b.min.y),
                a.min.z.min(b.min.z),
            ),
            Vector3::new(
                a.max.x.max(b.max.x),
                a.max.y.max(b.max.y),
                a.max.z.max(b.max.z),
            ),
        )
    }

    /// Vanilla `PistonMovingBlockEntity.getCollisionShape`, in world coordinates: what is left
    /// of the base block (an extended piston while its head retracts) plus the moving block
    /// where it is now. While pistons move an entity, the blocks moving the same way do not
    /// collide with it (vanilla `NOCLIP`).
    #[must_use]
    pub fn collision_shapes(&self) -> Vec<BoundingBox> {
        let block = Block::from_state_id(self.pushed_block_state.id);
        let mut shapes: Vec<BoundingBox> = Vec::new();
        if !self.extending
            && self.source
            && (block == &Block::PISTON || block == &Block::STICKY_PISTON)
        {
            let mut base = PistonProps::from_state_id(self.pushed_block_state.id);
            base.extended = true;
            let base = BlockState::from_id(base.to_state_id(block));
            shapes.extend(
                base.get_block_collision_shapes()
                    .map(|shape| shape.at_pos(self.position)),
            );
        }
        let progress = self.current_progress.load();
        if progress < 1.0 && NOCLIP.get() == Some(self.movement_direction()) {
            return shapes;
        }
        let moving = if self.source {
            let mut head = PistonHeadLikeProperties::default(&Block::PISTON_HEAD);
            head.facing = self.facing.to_facing();
            head.short = self.extending != (1.0 - progress < 0.25);
            BlockState::from_id(head.to_state_id(&Block::PISTON_HEAD))
        } else {
            self.pushed_block_state
        };
        shapes.extend(
            moving
                .get_block_collision_shapes()
                .map(|shape| self.at_progress(shape)),
        );
        shapes
    }

    /// Vanilla `moveCollidedEntities`: entities in the way of the moving block are pushed
    /// along (through `Entity.move(PISTON)`, so they still collide with other blocks), and a
    /// slime block also launches them.
    fn push_entities(&self, world: &Arc<World>, new_progress: f32) {
        let dir = self.movement_direction();
        let delta = f64::from(new_progress - self.current_progress.load());
        let state = self.collision_related_state();
        let shapes: Vec<BoundingBox> = state
            .get_block_collision_shapes()
            .map(|shape| self.at_progress(shape))
            .collect();
        let Some(bounds) = shapes.iter().copied().reduce(Self::union) else {
            return;
        };
        let area = Self::union(Self::movement_area(bounds, dir, delta), bounds);
        let entities = world.get_entities_at_box(&area);
        if entities.is_empty() {
            return;
        }
        let slime = Block::from_state_id(self.pushed_block_state.id) == &Block::SLIME_BLOCK;
        for entity in entities {
            if Self::ignores_pistons(entity.as_ref()) {
                continue;
            }
            let base = entity.get_entity();
            if slime && Self::can_simulate_movement(entity.as_ref()) {
                let mut velocity = base.velocity.load();
                let step = dir.to_offset();
                match dir.to_axis() {
                    Axis::X => velocity.x = f64::from(step.x),
                    Axis::Y => velocity.y = f64::from(step.y),
                    Axis::Z => velocity.z = f64::from(step.z),
                }
                base.velocity.store(velocity);
            }
            let entity_box = base.bounding_box.load();
            let mut movement: f64 = 0.0;
            for shape in &shapes {
                let swept = Self::movement_area(*shape, dir, delta);
                if !swept.intersects(&entity_box) {
                    continue;
                }
                movement = movement.max(Self::intersection_size(swept, dir, entity_box));
                if movement >= delta {
                    break;
                }
            }
            if movement <= 0.0 {
                continue;
            }
            let movement = movement.min(delta) + 0.01;
            Self::move_entity_by_piston(dir, entity.as_ref(), movement);
            if !self.extending && self.source {
                Self::fix_entity_within_piston_base(&self.position, entity.as_ref(), dir, delta);
            }
        }
    }

    /// Vanilla `moveStuckEntities`: a honey block moving sideways carries the entities
    /// standing on it.
    fn move_stuck_entities(&self, world: &Arc<World>, new_progress: f32) {
        if Block::from_state_id(self.pushed_block_state.id) != &Block::HONEY_BLOCK {
            return;
        }
        let dir = self.movement_direction();
        if !dir.is_horizontal() {
            return;
        }
        let top = self
            .pushed_block_state
            .get_block_collision_shapes()
            .map(|shape| shape.max.y)
            .fold(f64::NEG_INFINITY, f64::max);
        if !top.is_finite() {
            return;
        }
        let area = self.at_progress(BoundingBox::new(
            Vector3::new(0.0, top, 0.0),
            Vector3::new(1.0, 1.500_001_000_000_000_1, 1.0),
        ));
        let delta = f64::from(new_progress - self.current_progress.load());
        for entity in world.get_entities_at_box(&area) {
            if Self::ignores_pistons(entity.as_ref()) {
                continue;
            }
            // `matchesStickyCritera`
            let base = entity.get_entity();
            let pos = base.pos.load();
            let stuck = base.on_ground.load(Ordering::Relaxed)
                && (base.supporting_block_pos.load() == Some(self.position)
                    || (pos.x >= area.min.x
                        && pos.x <= area.max.x
                        && pos.z >= area.min.z
                        && pos.z <= area.max.z));
            if stuck {
                Self::move_entity_by_piston(dir, entity.as_ref(), delta);
            }
        }
    }

    /// Vanilla `PushReaction.IGNORE_ENTITY`.
    pub(crate) fn ignores_pistons(entity: &dyn EntityBase) -> bool {
        let entity_type = entity.get_entity().entity_type;
        if entity_type == &EntityType::ARMOR_STAND {
            return entity
                .cast_any()
                .downcast_ref::<crate::entity::decoration::armor_stand::ArmorStandEntity>()
                .is_some_and(crate::entity::decoration::armor_stand::ArmorStandEntity::is_marker);
        }
        [
            &EntityType::INTERACTION,
            &EntityType::BLOCK_DISPLAY,
            &EntityType::ITEM_DISPLAY,
            &EntityType::TEXT_DISPLAY,
            &EntityType::AREA_EFFECT_CLOUD,
            &EntityType::OMINOUS_ITEM_SPAWNER,
            &EntityType::MARKER,
        ]
        .contains(&entity_type)
    }

    /// Vanilla `canSimulateMovement` on the server: false for a vehicle a player steers.
    fn can_simulate_movement(entity: &dyn EntityBase) -> bool {
        entity
            .get_controlling_passenger()
            .is_none_or(|passenger| passenger.get_player().is_none())
    }

    /// Vanilla `moveEntityByPiston`.
    fn move_entity_by_piston(dir: BlockDirection, entity: &dyn EntityBase, amount: f64) {
        NOCLIP.set(Some(dir));
        entity
            .get_entity()
            .move_by_piston(entity, Self::dir_vec(dir, amount));
        // `applyEffectsFromBlocks`: what the entity was pushed into acts on it now
        entity.get_entity().tick_block_collisions(entity);
        NOCLIP.set(None);
    }

    /// Vanilla `getMovement`: how much `entity` overlaps `swept` along `motion_dir`.
    fn intersection_size(
        swept: BoundingBox,
        motion_dir: BlockDirection,
        entity: BoundingBox,
    ) -> f64 {
        match motion_dir {
            BlockDirection::East => swept.max.x - entity.min.x,
            BlockDirection::West => entity.max.x - swept.min.x,
            BlockDirection::Up => swept.max.y - entity.min.y,
            BlockDirection::Down => entity.max.y - swept.min.y,
            BlockDirection::South => swept.max.z - entity.min.z,
            BlockDirection::North => entity.max.z - swept.min.z,
        }
    }

    /// Vanilla `fixEntityWithinPistonBase`: when a piston head retracts, entities that ended
    /// up inside the piston base are pushed back out.
    fn fix_entity_within_piston_base(
        piston_pos: &BlockPos,
        entity: &dyn EntityBase,
        motion_dir: BlockDirection,
        amount: f64,
    ) {
        let body_aabb = BoundingBox::from_block(piston_pos);
        let entity_aabb = entity.get_entity().bounding_box.load();
        if !body_aabb.intersects(&entity_aabb) {
            return;
        }
        let back = motion_dir.opposite();
        let e = Self::intersection_size(body_aabb, back, entity_aabb) + 0.01;
        let f = Self::intersection_size(
            body_aabb,
            back,
            Self::aabb_intersection(body_aabb, entity_aabb),
        ) + 0.01;
        if (e - f).abs() < 0.01 {
            let distance = e.min(amount) + 0.01;
            Self::move_entity_by_piston(back, entity, distance);
        }
    }

    const fn aabb_intersection(a: BoundingBox, b: BoundingBox) -> BoundingBox {
        BoundingBox::new(
            Vector3::new(
                a.min.x.max(b.min.x),
                a.min.y.max(b.min.y),
                a.min.z.max(b.min.z),
            ),
            Vector3::new(
                a.max.x.min(b.max.x),
                a.max.y.min(b.max.y),
                a.max.z.min(b.max.z),
            ),
        )
    }

    pub fn finish(&self, world: &Arc<World>) {
        if self.last_progress.load() < 1.0 {
            let pos = self.position;
            world.remove_block_entity(&pos);
            if world.get_block(&pos) == &Block::MOVING_PISTON {
                let state = if self.source {
                    Block::AIR.default_state.id
                } else {
                    world.update_from_neighbor_shapes(self.pushed_block_state.id, &pos)
                };
                // Vanilla `setBlockAndUpdate` then `neighborChanged` on the block itself.
                world.set_block_state(&pos, state, BlockFlags::NOTIFY_ALL);
                world.update_neighbor(&pos, Block::from_state_id(state));
            }
        }
    }
}

const BLOCK_STATE: &str = "blockState";
const FACING: &str = "facing";
const LAST_PROGRESS: &str = "progress";
const EXTENDING: &str = "extending";
const SOURCE: &str = "source";

impl BlockEntity for PistonBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn tick(&self, world: &Arc<World>) {
        self.last_ticked
            .store(world.get_world_age(), Ordering::Relaxed);
        let current_progress = self.current_progress.load();
        self.last_progress.store(current_progress);
        if current_progress >= 1.0 {
            let pos = self.position;
            world.remove_block_entity(&pos);
            if world.get_block(&pos) == &Block::MOVING_PISTON {
                // Vanilla `PistonMovingBlockEntity.tick`: a block that cannot stay where it
                // arrived is placed (flag 340) and then destroyed with drops (`updateOrDestroy`).
                let updated_state =
                    world.update_from_neighbor_shapes(self.pushed_block_state.id, &pos);
                if is_air(updated_state) {
                    world.set_block_state(
                        &pos,
                        self.pushed_block_state.id,
                        // Vanilla flags 340: no shape updates.
                        BlockFlags::FORCE_STATE
                            | BlockFlags::MOVED
                            | BlockFlags::UPDATE_KNOWN_SHAPE,
                    );
                    if !self.pushed_block_state.is_air() {
                        world.break_block(&pos, None, BlockFlags::NOTIFY_ALL);
                    }
                } else {
                    let updated_state = clear_waterlogged(updated_state);
                    world.set_block_state(
                        &pos,
                        updated_state,
                        BlockFlags::NOTIFY_ALL | BlockFlags::MOVED,
                    );
                    world.update_neighbor(&pos, Block::from_state_id(updated_state));
                }
            }
            return;
        }
        let new_progress = current_progress + 0.5;
        self.push_entities(world, new_progress);
        self.move_stuck_entities(world, new_progress);
        self.current_progress.store(new_progress.min(1.0));
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        // Vanilla `blockState`, written with `NbtUtils.writeBlockState`.
        let pushed_block_state = nbt
            .get_compound(BLOCK_STATE)
            .and_then(pumpkin_world::block::BlockStateCodec::state_from_nbt)
            .map_or(Block::AIR.default_state, BlockState::from_id);
        // Vanilla writes `facing` as an int; older Pumpkin saves used a byte.
        let facing = nbt
            .get_int(FACING)
            .or_else(|| nbt.get_byte(FACING).map(i32::from))
            .unwrap_or(0);
        let last_progress = nbt.get_float(LAST_PROGRESS).unwrap_or(0.0);
        let extending = nbt.get_bool(EXTENDING).unwrap_or(false);
        let source = nbt.get_bool(SOURCE).unwrap_or(false);
        Self {
            pushed_block_state,
            position,
            facing: BlockDirection::from_index(facing as u8).unwrap_or(BlockDirection::Down),
            current_progress: last_progress.into(),
            last_progress: last_progress.into(),
            extending,
            source,
            last_ticked: AtomicI64::new(0),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_compound(
            BLOCK_STATE,
            pumpkin_world::block::BlockStateCodec::state_to_nbt(self.pushed_block_state.id),
        );
        nbt.put_int(FACING, i32::from(self.facing.to_index()));
        nbt.put_float(LAST_PROGRESS, self.last_progress.load());
        nbt.put_bool(EXTENDING, self.extending);
        nbt.put_bool(SOURCE, self.source);
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.write_nbt(&mut nbt);
        Some(nbt)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Vanilla `PistonMovingBlockEntity.tick`: a moved block arrives without its water.
fn clear_waterlogged(state_id: pumpkin_data::BlockStateId) -> pumpkin_data::BlockStateId {
    use pumpkin_data::block_properties::Property;
    let block = Block::from_state_id(state_id);
    let Some(props) = block.properties(state_id) else {
        return state_id;
    };
    let mut props = props.to_props();
    let mut changed = false;
    for prop in &mut props {
        if *prop == Property::Waterlogged(true) {
            *prop = Property::Waterlogged(false);
            changed = true;
        }
    }
    if changed {
        block.from_properties(&props).to_state_id(block)
    } else {
        state_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vanilla saves the moved block as `blockState` and `facing` as an int.
    #[test]
    fn moving_block_state_round_trips() {
        let state = Block::OAK_LOG.default_state;
        let piston = PistonBlockEntity {
            position: BlockPos::new(1, 64, 1),
            pushed_block_state: state,
            facing: BlockDirection::East,
            current_progress: 0.5.into(),
            last_progress: 0.5.into(),
            extending: true,
            source: false,
            last_ticked: AtomicI64::new(0),
        };
        let mut nbt = NbtCompound::new();
        piston.write_nbt(&mut nbt);
        assert_eq!(
            nbt.get_int("facing"),
            Some(i32::from(BlockDirection::East.to_index()))
        );
        let loaded = PistonBlockEntity::from_nbt(&nbt, BlockPos::new(1, 64, 1));
        assert_eq!(loaded.pushed_block_state.id, state.id);
        assert_eq!(loaded.facing, BlockDirection::East);
        assert!(loaded.extending);
    }
}
