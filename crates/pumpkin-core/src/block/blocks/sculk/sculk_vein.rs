use rustc_hash::FxHashSet;

use crate::block::{
    BlockBehaviour, BlockIsReplacing, BlockMetadata, BonemealArgs, CanPlaceAtArgs, CanUpdateAtArgs,
    GetStateForNeighborUpdateArgs, OnPlaceArgs,
};
use crate::entity::{EntityBase, player::Player};
use pumpkin_data::fluid::Fluid;
use pumpkin_data::{
    Block, BlockDirection, BlockId, BlockStateId, FacingExt,
    block_properties::GlowLichenLikeProperties,
};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockAccessor;

pub struct MultifaceBlock;

impl BlockMetadata for MultifaceBlock {
    fn ids() -> Box<[BlockId]> {
        [
            BlockId::SCULK_VEIN,
            BlockId::GLOW_LICHEN,
            BlockId::RESIN_CLUMP,
        ]
        .into()
    }
}

impl BlockBehaviour for MultifaceBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if let BlockIsReplacing::Itself(state_id) = args.replacing {
            let (Some(direction), _) = get_attach_direction(
                args.world,
                args.position,
                args.block,
                Some(args.player),
                args.direction,
                true,
            ) else {
                return Block::AIR.default_state.id;
            };
            let mut props = GlowLichenLikeProperties::from_state_id(state_id);
            set_face(&mut props, direction);
            return props.to_state_id(args.block);
        }
        let (Some(direction), _) = get_attach_direction(
            args.world,
            args.position,
            args.block,
            Some(args.player),
            args.direction,
            false,
        ) else {
            return Block::AIR.default_state.id;
        };
        let mut props = GlowLichenLikeProperties::default(args.block);
        set_face(&mut props, direction);
        props.waterlogged = args.replacing.water_source();
        props.to_state_id(args.block)
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        get_attach_direction(
            args.block_accessor,
            args.position,
            args.block,
            args.player,
            args.direction.unwrap_or(BlockDirection::Down),
            false,
        )
        .0
        .is_some()
    }

    fn can_update_at(&self, args: CanUpdateAtArgs<'_>) -> bool {
        get_attach_direction(
            args.world,
            args.position,
            args.block,
            Some(args.player),
            args.direction,
            true,
        )
        .0
        .is_some()
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        let old_props = GlowLichenLikeProperties::from_state_id(args.state_id);
        if old_props.waterlogged {
            args.world.schedule_fluid_tick(
                &Fluid::WATER,
                *args.position,
                Fluid::WATER.flow_speed as u8,
                TickPriority::Normal,
            );
        }

        let mut new_directions = active_directions(old_props);
        if !can_attach_to(args.world, args.neighbor_position, args.direction) {
            new_directions.remove(&args.direction);
        }

        if new_directions.is_empty() {
            return Block::AIR.default_state.id;
        }
        let mut new_props = GlowLichenLikeProperties::default(args.block);
        for dir in new_directions {
            set_face(&mut new_props, dir);
        }
        new_props.waterlogged = old_props.waterlogged;
        new_props.to_state_id(args.block)
    }

    /// Vanilla `GlowLichenBlock.isValidBonemealTarget`.
    fn is_valid_bonemeal_target(&self, args: BonemealArgs<'_>) -> bool {
        if args.block != &Block::GLOW_LICHEN {
            return false;
        }
        let spreader = super::multiface_spreader::MultifaceSpreader::new(&Block::GLOW_LICHEN);
        BlockDirection::all().into_iter().any(|direction| {
            spreader.can_spread_in_any_direction(
                args.world,
                args.position,
                args.state_id,
                direction.opposite(),
            )
        })
    }

    /// Vanilla `GlowLichenBlock.performBonemeal`: grow onto one neighbouring face.
    fn perform_bonemeal(&self, args: BonemealArgs<'_>) {
        if args.block != &Block::GLOW_LICHEN {
            return;
        }
        super::multiface_spreader::MultifaceSpreader::new(&Block::GLOW_LICHEN)
            .spread_from_random_face_toward_random_direction(
                args.world,
                args.position,
                args.state_id,
                &mut rand::rng(),
            );
    }
}

fn get_attach_direction(
    block_accessor: &dyn BlockAccessor,
    block_pos: &BlockPos,
    target_block: &Block,
    player_wrapper: Option<&Player>,
    click_direction: BlockDirection,
    replacing: bool,
) -> (Option<BlockDirection>, bool) {
    let clicked_pos = block_pos.offset(click_direction.to_offset());
    let clicked_block = block_accessor.get_block(&clicked_pos);

    if !replacing && clicked_block == target_block {
        return (None, false);
    }

    if can_attach_to(block_accessor, &clicked_pos, click_direction) {
        return (Some(click_direction), false);
    }

    let (replacing_block, replacing_block_state) = block_accessor.get_block_and_state(block_pos);
    let already_active = if replacing_block == target_block {
        active_directions(GlowLichenLikeProperties::from_state_id(
            replacing_block_state.id,
        ))
    } else {
        FxHashSet::default()
    };

    if let Some(player) = player_wrapper {
        let fs = player.get_entity().get_entity_facing_order();
        let directions = [
            fs[0].to_block_direction(),
            fs[1].to_block_direction(),
            fs[2].to_block_direction(),
            fs[3].to_block_direction(),
            fs[4].to_block_direction(),
            fs[5].to_block_direction(),
        ];
        for dir in directions {
            if !already_active.contains(&dir)
                && can_attach_to(block_accessor, &block_pos.offset(dir.to_offset()), dir)
            {
                return (Some(dir), false);
            }
        }
    }
    (None, false)
}

/// Vanilla `MultifaceBlock.canAttachTo`: the support's face towards the vein is sturdy.
fn can_attach_to(
    block_accessor: &dyn BlockAccessor,
    support_pos: &BlockPos,
    direction: BlockDirection,
) -> bool {
    block_accessor.is_face_sturdy(support_pos, direction.opposite())
}

fn active_directions(props: GlowLichenLikeProperties) -> FxHashSet<BlockDirection> {
    let mut set = FxHashSet::default();
    if props.down {
        set.insert(BlockDirection::Down);
    }
    if props.up {
        set.insert(BlockDirection::Up);
    }
    if props.north {
        set.insert(BlockDirection::North);
    }
    if props.south {
        set.insert(BlockDirection::South);
    }
    if props.east {
        set.insert(BlockDirection::East);
    }
    if props.west {
        set.insert(BlockDirection::West);
    }
    set
}

const fn set_face(props: &mut GlowLichenLikeProperties, direction: BlockDirection) {
    match direction {
        BlockDirection::Down => props.down = true,
        BlockDirection::Up => props.up = true,
        BlockDirection::North => props.north = true,
        BlockDirection::South => props.south = true,
        BlockDirection::West => props.west = true,
        BlockDirection::East => props.east = true,
    }
}
