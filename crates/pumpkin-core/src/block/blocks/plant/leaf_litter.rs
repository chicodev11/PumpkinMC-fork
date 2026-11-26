use pumpkin_data::BlockStateId;
use pumpkin_data::{Block, BlockDirection};
use pumpkin_macros::pumpkin_block;

use crate::block::{
    BlockBehaviour, CanPlaceAtArgs, CanUpdateAtArgs, GetStateForNeighborUpdateArgs, OnPlaceArgs,
};

use super::segmented::Segmented;

type LeafLitterProperties = pumpkin_data::block_properties::LeafLitterLikeProperties;

#[pumpkin_block("minecraft:leaf_litter")]
pub struct LeafLitterBlock;

impl BlockBehaviour for LeafLitterBlock {
    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        args.block_accessor
            .is_face_sturdy(&args.position.down(), BlockDirection::Up)
    }

    fn can_update_at(&self, args: CanUpdateAtArgs<'_>) -> bool {
        Segmented::can_update_at(self, args)
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        Segmented::on_place(self, args)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if args.direction == BlockDirection::Down
            && !args
                .world
                .is_face_sturdy(&args.position.down(), BlockDirection::Up)
        {
            return Block::AIR.default_state.id;
        }
        args.state_id
    }
}

impl Segmented for LeafLitterBlock {
    type Properties = LeafLitterProperties;
}
