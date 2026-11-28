use crate::block::blocks::redstone::block_receives_redstone_power;
use crate::block::entities::skull::SkullBlockEntity;
use crate::block::{
    BlockBehaviour, BlockMetadata, OnNeighborUpdateArgs, OnPlaceArgs, PathComputationType,
    PlacedArgs,
};
use crate::entity::EntityBase;
use pumpkin_data::block_properties::{
    Facing, SkeletonSkullLikeProperties, SkeletonWallSkullLikeProperties,
};
use pumpkin_data::{Block, BlockId, BlockState, BlockStateId, FacingExt};
use pumpkin_world::world::BlockFlags;
use std::sync::Arc;

pub struct SkullBlock;

const fn is_wall_skull(block: &Block) -> bool {
    matches!(
        block.id,
        BlockId::SKELETON_WALL_SKULL
            | BlockId::WITHER_SKELETON_WALL_SKULL
            | BlockId::PLAYER_WALL_HEAD
            | BlockId::ZOMBIE_WALL_HEAD
            | BlockId::CREEPER_WALL_HEAD
            | BlockId::PIGLIN_WALL_HEAD
            | BlockId::DRAGON_WALL_HEAD
    )
}

const fn get_wall_skull_block(standing_block: &Block) -> &Block {
    match standing_block.id {
        BlockId::SKELETON_SKULL => &Block::SKELETON_WALL_SKULL,
        BlockId::WITHER_SKELETON_SKULL => &Block::WITHER_SKELETON_WALL_SKULL,
        BlockId::PLAYER_HEAD => &Block::PLAYER_WALL_HEAD,
        BlockId::ZOMBIE_HEAD => &Block::ZOMBIE_WALL_HEAD,
        BlockId::CREEPER_HEAD => &Block::CREEPER_WALL_HEAD,
        BlockId::PIGLIN_HEAD => &Block::PIGLIN_WALL_HEAD,
        BlockId::DRAGON_HEAD => &Block::DRAGON_WALL_HEAD,
        _ => standing_block,
    }
}

impl BlockMetadata for SkullBlock {
    fn ids() -> Box<[BlockId]> {
        [
            BlockId::SKELETON_SKULL,
            BlockId::SKELETON_WALL_SKULL,
            BlockId::PLAYER_HEAD,
            BlockId::PLAYER_WALL_HEAD,
            BlockId::ZOMBIE_HEAD,
            BlockId::ZOMBIE_WALL_HEAD,
            BlockId::CREEPER_HEAD,
            BlockId::CREEPER_WALL_HEAD,
            BlockId::PIGLIN_HEAD,
            BlockId::PIGLIN_WALL_HEAD,
            BlockId::DRAGON_HEAD,
            BlockId::DRAGON_WALL_HEAD,
        ]
        .into()
    }
}

impl BlockBehaviour for SkullBlock {
    fn placed(&self, args: PlacedArgs<'_>) {
        let entity = SkullBlockEntity::new(*args.position);
        args.world.add_block_entity(Arc::new(entity));
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let powered = block_receives_redstone_power(args.world, args.position);

        // Vanilla `StandingAndWallBlockItem.getPlacementState` (attached DOWN): the wall state is
        // the first horizontal nearest looking direction whose neighbour is not replaceable
        // (`WallSkullBlock.getStateForPlacement`); walking the same order, DOWN picks the standing
        // skull (which always survives) and any horizontal direction picks that wall state.
        let directions = args.nearest_looking_directions();
        let wall_facing = directions.iter().copied().find(|dir| {
            dir.to_horizontal_facing().is_some()
                && !args
                    .world
                    .get_block_state(&args.position.offset(dir.to_block_direction().to_offset()))
                    .replaceable()
        });

        for dir in directions {
            match dir {
                Facing::Up => {}
                Facing::Down => break,
                _ => {
                    if let Some(facing) =
                        wall_facing.and_then(|d| d.opposite().to_horizontal_facing())
                    {
                        let wall_block = get_wall_skull_block(args.block);
                        let mut props = SkeletonWallSkullLikeProperties::default(wall_block);
                        props.facing = facing;
                        props.powered = powered;
                        return props.to_state_id(wall_block);
                    }
                }
            }
        }

        let mut props = SkeletonSkullLikeProperties::default(args.block);
        props.rotation = args.player.get_entity().get_rotation_16();
        props.powered = powered;
        props.to_state_id(args.block)
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        let state = args.world.get_block_state(args.position);
        let is_receiving_power = block_receives_redstone_power(args.world, args.position);
        if is_wall_skull(args.block) {
            let mut props = SkeletonWallSkullLikeProperties::from_state_id(state.id);
            if props.powered != is_receiving_power {
                props.powered = is_receiving_power;
                args.world.set_block_state(
                    args.position,
                    props.to_state_id(args.block),
                    BlockFlags::NOTIFY_LISTENERS,
                );
            }
        } else {
            let mut props = SkeletonSkullLikeProperties::from_state_id(state.id);
            if props.powered != is_receiving_power {
                props.powered = is_receiving_power;
                args.world.set_block_state(
                    args.position,
                    props.to_state_id(args.block),
                    BlockFlags::NOTIFY_LISTENERS,
                );
            }
        }
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}
