//! Vanilla `MultifaceSpreader`: how sculk veins and glow lichen grow onto neighbouring faces.

use pumpkin_data::block_properties::GlowLichenLikeProperties;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::{Block, BlockDirection, BlockStateId};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;
use rand::seq::SliceRandom;
use std::sync::Arc;

use crate::world::World;

#[derive(Clone, Copy)]
pub enum SpreadType {
    SamePosition,
    SamePlane,
    WrapAround,
}

impl SpreadType {
    fn spread_pos(self, pos: BlockPos, to: BlockDirection, from_face: BlockDirection) -> SpreadPos {
        match self {
            Self::SamePosition => SpreadPos { pos, face: to },
            Self::SamePlane => SpreadPos {
                pos: pos.offset(to.to_offset()),
                face: from_face,
            },
            Self::WrapAround => SpreadPos {
                pos: pos.offset(to.to_offset()).offset(from_face.to_offset()),
                face: to.opposite(),
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct SpreadPos {
    pub pos: BlockPos,
    pub face: BlockDirection,
}

pub const DEFAULT_SPREAD_ORDER: &[SpreadType] = &[
    SpreadType::SamePosition,
    SpreadType::SamePlane,
    SpreadType::WrapAround,
];

pub struct MultifaceSpreader {
    block: &'static Block,
    spread_types: &'static [SpreadType],
    /// Vanilla `SculkVeinSpreaderConfig`.
    sculk_vein: bool,
}

/// Vanilla `MultifaceBlock.hasFace`.
#[must_use]
pub fn has_face(state: BlockStateId, direction: BlockDirection) -> bool {
    let props = GlowLichenLikeProperties::from_state_id(state);
    match direction {
        BlockDirection::Down => props.down,
        BlockDirection::Up => props.up,
        BlockDirection::North => props.north,
        BlockDirection::South => props.south,
        BlockDirection::West => props.west,
        BlockDirection::East => props.east,
    }
}

pub const fn set_face(
    props: &mut GlowLichenLikeProperties,
    direction: BlockDirection,
    value: bool,
) {
    match direction {
        BlockDirection::Down => props.down = value,
        BlockDirection::Up => props.up = value,
        BlockDirection::North => props.north = value,
        BlockDirection::South => props.south = value,
        BlockDirection::West => props.west = value,
        BlockDirection::East => props.east = value,
    }
}

/// Vanilla `MultifaceBlock.canAttachTo`.
#[must_use]
pub fn can_attach_to(world: &World, pos: &BlockPos, direction: BlockDirection) -> bool {
    world.is_face_sturdy(&pos.offset(direction.to_offset()), direction.opposite())
}

fn is_water_source(world: &World, pos: &BlockPos) -> bool {
    let (fluid, state) = world.get_fluid_and_fluid_state(pos);
    fluid.matches_type(&Fluid::WATER) && state.is_source
}

impl MultifaceSpreader {
    #[must_use]
    pub const fn new(block: &'static Block) -> Self {
        Self {
            block,
            spread_types: DEFAULT_SPREAD_ORDER,
            sculk_vein: false,
        }
    }

    /// Vanilla `SculkVeinBlock.veinSpreader` / `sameSpaceSpreader`.
    #[must_use]
    pub const fn sculk_vein(spread_types: &'static [SpreadType]) -> Self {
        Self {
            block: &Block::SCULK_VEIN,
            spread_types,
            sculk_vein: true,
        }
    }

    fn is_other_block_valid_as_source(&self, state: BlockStateId) -> bool {
        self.sculk_vein && state.to_block() != &Block::SCULK_VEIN
    }

    fn holds(&self, state: BlockStateId) -> bool {
        state.to_block() == self.block
    }

    fn can_spread_from(&self, state: BlockStateId, direction: BlockDirection) -> bool {
        self.is_other_block_valid_as_source(state)
            || (self.holds(state) && has_face(state, direction))
    }

    fn state_can_be_replaced(
        &self,
        world: &World,
        pos: &BlockPos,
        spread_pos: &BlockPos,
        direction: BlockDirection,
        state: BlockStateId,
    ) -> bool {
        let block = state.to_block();
        let default = || {
            world.get_block_state(spread_pos).is_air()
                || block == self.block
                || (block == &Block::WATER && is_water_source(world, spread_pos))
        };
        if !self.sculk_vein {
            return default();
        }
        let behind = world.get_block(&spread_pos.offset(direction.to_offset()));
        if behind == &Block::SCULK
            || behind == &Block::SCULK_CATALYST
            || behind == &Block::MOVING_PISTON
        {
            return false;
        }
        if pos.manhattan_distance(*spread_pos) == 2 {
            let back = pos.offset(direction.opposite().to_offset());
            if world.is_face_sturdy(&back, direction) {
                return false;
            }
        }
        let (fluid, fluid_state) = world.get_fluid_and_fluid_state(spread_pos);
        if !fluid_state.is_empty && !fluid.matches_type(&Fluid::WATER) {
            return false;
        }
        if block == &Block::FIRE {
            return false;
        }
        world.get_block_state(spread_pos).replaceable() || default()
    }

    /// Vanilla `MultifaceBlock.isValidStateForPlacement`.
    fn is_valid_state_for_placement(
        &self,
        world: &World,
        state: BlockStateId,
        pos: &BlockPos,
        face: BlockDirection,
    ) -> bool {
        (!self.holds(state) || !has_face(state, face)) && can_attach_to(world, pos, face)
    }

    fn can_spread_into(&self, world: &World, pos: &BlockPos, spread: &SpreadPos) -> bool {
        let state = world.get_block_state_id(&spread.pos);
        self.state_can_be_replaced(world, pos, &spread.pos, spread.face, state)
            && self.is_valid_state_for_placement(world, state, &spread.pos, spread.face)
    }

    /// Vanilla `MultifaceBlock.getStateForPlacement`.
    fn state_for_placement(
        &self,
        world: &World,
        current: BlockStateId,
        pos: &BlockPos,
        face: BlockDirection,
    ) -> Option<BlockStateId> {
        if !self.is_valid_state_for_placement(world, current, pos, face) {
            return None;
        }
        let mut props = if self.holds(current) {
            GlowLichenLikeProperties::from_state_id(current)
        } else {
            let mut props = GlowLichenLikeProperties::from_state_id(self.block.default_state.id);
            props.waterlogged = is_water_source(world, pos);
            props
        };
        set_face(&mut props, face, true);
        Some(props.to_state_id(self.block))
    }

    fn spread_to_face(&self, world: &Arc<World>, spread: SpreadPos) -> Option<SpreadPos> {
        let current = world.get_block_state_id(&spread.pos);
        let state = self.state_for_placement(world, current, &spread.pos, spread.face)?;
        world.set_block_state(&spread.pos, state, BlockFlags::NOTIFY_LISTENERS);
        Some(spread)
    }

    fn spread_from_face_toward_direction(
        &self,
        world: &Arc<World>,
        pos: &BlockPos,
        state: BlockStateId,
        from_face: BlockDirection,
        to: BlockDirection,
    ) -> Option<SpreadPos> {
        if to.to_axis() == from_face.to_axis() {
            return None;
        }
        if !self.is_other_block_valid_as_source(state)
            && (!self.holds(state) || !has_face(state, from_face) || has_face(state, to))
        {
            return None;
        }
        for spread_type in self.spread_types {
            let spread = spread_type.spread_pos(*pos, to, from_face);
            if self.can_spread_into(world, pos, &spread) {
                return self.spread_to_face(world, spread);
            }
        }
        None
    }

    /// Vanilla `spreadAll`: grow from every face toward every direction.
    pub fn spread_all(&self, world: &Arc<World>, pos: &BlockPos, state: BlockStateId) -> u64 {
        let mut count = 0;
        for from_face in BlockDirection::all() {
            if !self.can_spread_from(state, from_face) {
                continue;
            }
            for to in BlockDirection::all() {
                if self
                    .spread_from_face_toward_direction(world, pos, state, from_face, to)
                    .is_some()
                {
                    count += 1;
                }
            }
        }
        count
    }

    /// Vanilla `spreadFromRandomFaceTowardRandomDirection`.
    pub fn spread_from_random_face_toward_random_direction(
        &self,
        world: &Arc<World>,
        pos: &BlockPos,
        state: BlockStateId,
        rng: &mut impl rand::Rng,
    ) -> Option<SpreadPos> {
        let mut faces = BlockDirection::all();
        faces.shuffle(rng);
        for from_face in faces {
            if !self.can_spread_from(state, from_face) {
                continue;
            }
            let mut directions = BlockDirection::all();
            directions.shuffle(rng);
            for to in directions {
                if let Some(spread) =
                    self.spread_from_face_toward_direction(world, pos, state, from_face, to)
                {
                    return Some(spread);
                }
            }
        }
        None
    }

    /// Vanilla `canSpreadInAnyDirection`, for glow lichen's bonemeal check.
    #[must_use]
    pub fn can_spread_in_any_direction(
        &self,
        world: &World,
        pos: &BlockPos,
        state: BlockStateId,
        from_face: BlockDirection,
    ) -> bool {
        BlockDirection::all().into_iter().any(|to| {
            to.to_axis() != from_face.to_axis()
                && (self.is_other_block_valid_as_source(state)
                    || (self.holds(state) && has_face(state, from_face) && !has_face(state, to)))
                && self.spread_types.iter().any(|spread_type| {
                    self.can_spread_into(world, pos, &spread_type.spread_pos(*pos, to, from_face))
                })
        })
    }
}
