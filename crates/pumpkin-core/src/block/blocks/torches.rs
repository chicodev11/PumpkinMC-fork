use crate::block::{
    BlockBehaviour, BlockMetadata, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnPlaceArgs,
    can_support_center,
};
use pumpkin_data::BlockStateId;
use pumpkin_data::block_properties::Facing;
use pumpkin_data::{Block, FacingExt, HorizontalFacingExt};
use pumpkin_data::{BlockDirection, BlockId};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockAccessor;

type WallTorchProps = pumpkin_data::block_properties::WallTorchLikeProperties;
// Normal tourches don't have properties

pub struct TorchBlock;

impl BlockMetadata for TorchBlock {
    fn ids() -> Box<[BlockId]> {
        [
            BlockId::TORCH,
            BlockId::SOUL_TORCH,
            BlockId::WALL_TORCH,
            BlockId::SOUL_WALL_TORCH,
            BlockId::COPPER_TORCH,
            BlockId::COPPER_WALL_TORCH,
        ]
        .into()
    }
}

impl BlockBehaviour for TorchBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        // Vanilla `StandingAndWallBlockItem.getPlacementState` (attached DOWN): the wall state is
        // the first horizontal nearest looking direction that survives; walking the same order,
        // DOWN picks the standing torch and any horizontal direction picks that wall state.
        let directions = args.nearest_looking_directions();
        let wall_facing = directions.iter().copied().find(|dir| {
            dir.to_horizontal_facing().is_some()
                && can_place_at(args.world, args.position, dir.to_block_direction())
        });

        for dir in directions {
            match dir {
                Facing::Up => {}
                Facing::Down => {
                    if can_support_center(args.world, &args.position.down(), BlockDirection::Up) {
                        return args.block.default_state.id;
                    }
                }
                _ => {
                    if let Some(facing) =
                        wall_facing.and_then(|d| d.opposite().to_horizontal_facing())
                    {
                        let wall_block = if args.block == &Block::TORCH {
                            Block::WALL_TORCH
                        } else if args.block == &Block::SOUL_TORCH {
                            Block::SOUL_WALL_TORCH
                        } else {
                            Block::COPPER_WALL_TORCH
                        };
                        let mut torch_props = WallTorchProps::default(&wall_block);
                        torch_props.facing = facing;
                        return torch_props.to_state_id(&wall_block);
                    }
                }
            }
        }

        BlockStateId::AIR
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        if args.player.is_none() {
            if args.block == &Block::WALL_TORCH
                || args.block == &Block::SOUL_WALL_TORCH
                || args.block == &Block::COPPER_WALL_TORCH
            {
                let props = WallTorchProps::from_state_id(args.state.id);
                return can_place_at(
                    args.block_accessor,
                    args.position,
                    props.facing.to_block_direction().opposite(),
                );
            }
            return can_support_center(
                args.block_accessor,
                &args.position.down(),
                BlockDirection::Up,
            );
        }

        if can_support_center(
            args.block_accessor,
            &args.position.down(),
            BlockDirection::Up,
        ) {
            return true;
        }
        for dir in BlockDirection::horizontal() {
            if can_place_at(args.block_accessor, args.position, dir.to_block_direction()) {
                return true;
            }
        }
        false
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if args.block == &Block::WALL_TORCH
            || args.block == &Block::SOUL_WALL_TORCH
            || args.block == &Block::COPPER_WALL_TORCH
        {
            let props = WallTorchProps::from_state_id(args.state_id);
            if props.facing.to_block_direction().opposite() == args.direction
                && !can_place_at(
                    args.world,
                    args.position,
                    props.facing.to_block_direction().opposite(),
                )
            {
                return BlockStateId::AIR;
            }
        } else if args.direction == BlockDirection::Down
            && !can_support_center(args.world, &args.position.down(), BlockDirection::Up)
        {
            return BlockStateId::AIR;
        }
        args.state_id
    }
}

fn can_place_at(world: &dyn BlockAccessor, block_pos: &BlockPos, facing: BlockDirection) -> bool {
    world.is_face_sturdy(&block_pos.offset(facing.to_offset()), facing.opposite())
}
