use std::sync::Arc;

use pumpkin_data::{
    Block, BlockId, BlockState, BlockStateId, Rotation,
    block_properties::{Axis, PaleOakWoodLikeProperties},
    entity::{EntityStatus, EntityType},
};
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use crate::{
    block::{BlockBehaviour, BlockMetadata, OnPlaceArgs},
    entity::r#type::from_type,
    world::World,
};

pub struct InfestedBlock;

impl BlockMetadata for InfestedBlock {
    fn ids() -> Box<[BlockId]> {
        [
            BlockId::INFESTED_STONE,
            BlockId::INFESTED_COBBLESTONE,
            BlockId::INFESTED_STONE_BRICKS,
            BlockId::INFESTED_MOSSY_STONE_BRICKS,
            BlockId::INFESTED_CRACKED_STONE_BRICKS,
            BlockId::INFESTED_CHISELED_STONE_BRICKS,
            BlockId::INFESTED_DEEPSLATE,
        ]
        .into()
    }
}

impl BlockBehaviour for InfestedBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if args.block == &Block::INFESTED_DEEPSLATE {
            let mut props = PaleOakWoodLikeProperties::default(args.block);
            props.axis = args.direction.to_axis();
            props.to_state_id(args.block)
        } else {
            args.block.default_state.id
        }
    }

    fn rotate(
        &self,
        block: &Block,
        state_id: BlockStateId,
        rotation: Rotation,
    ) -> &'static BlockState {
        if block == &Block::INFESTED_DEEPSLATE {
            let mut props = PaleOakWoodLikeProperties::from_state_id(state_id);
            if matches!(
                rotation,
                Rotation::Clockwise90 | Rotation::CounterClockwise90
            ) {
                props.axis = match props.axis {
                    Axis::X => Axis::Z,
                    Axis::Z => Axis::X,
                    Axis::Y => Axis::Y,
                };
            }
            BlockState::from_id(props.to_state_id(block))
        } else {
            BlockState::from_id(state_id)
        }
    }
}

/// Infested blocks paired with the block they hide in.
const INFESTED_HOSTS: [(&Block, &Block); 7] = [
    (&Block::INFESTED_STONE, &Block::STONE),
    (&Block::INFESTED_COBBLESTONE, &Block::COBBLESTONE),
    (&Block::INFESTED_STONE_BRICKS, &Block::STONE_BRICKS),
    (
        &Block::INFESTED_MOSSY_STONE_BRICKS,
        &Block::MOSSY_STONE_BRICKS,
    ),
    (
        &Block::INFESTED_CRACKED_STONE_BRICKS,
        &Block::CRACKED_STONE_BRICKS,
    ),
    (
        &Block::INFESTED_CHISELED_STONE_BRICKS,
        &Block::CHISELED_STONE_BRICKS,
    ),
    (&Block::INFESTED_DEEPSLATE, &Block::DEEPSLATE),
];

/// Copies the deepslate axis across, the only property infested blocks have.
fn convert_state(state_id: BlockStateId, from: &Block, to: &Block) -> BlockStateId {
    if from == &Block::INFESTED_DEEPSLATE || from == &Block::DEEPSLATE {
        let props = PaleOakWoodLikeProperties::from_state_id(state_id);
        let mut target = PaleOakWoodLikeProperties::default(to);
        target.axis = props.axis;
        target.to_state_id(to)
    } else {
        to.default_state.id
    }
}

#[must_use]
pub fn is_infested(block: &Block) -> bool {
    INFESTED_HOSTS
        .iter()
        .any(|(infested, _)| *infested == block)
}

/// Vanilla `InfestedBlock.isCompatibleHostBlock`.
#[must_use]
pub fn is_compatible_host_block(block: &Block) -> bool {
    INFESTED_HOSTS.iter().any(|(_, host)| *host == block)
}

/// Vanilla `InfestedBlock.hostStateByInfested`.
#[must_use]
pub fn host_state_by_infested(block: &Block, state_id: BlockStateId) -> Option<BlockStateId> {
    INFESTED_HOSTS
        .iter()
        .find(|(infested, _)| *infested == block)
        .map(|(infested, host)| convert_state(state_id, infested, host))
}

/// Vanilla `InfestedBlock.infestedStateByHost`.
#[must_use]
pub fn infested_state_by_host(block: &Block, state_id: BlockStateId) -> Option<BlockStateId> {
    INFESTED_HOSTS
        .iter()
        .find(|(_, host)| *host == block)
        .map(|(infested, host)| convert_state(state_id, host, infested))
}

/// Vanilla `InfestedBlock.spawnInfestation`: a silverfish crawls out of the broken block.
pub fn spawn_infestation(world: &Arc<World>, position: &BlockPos) {
    let pos = Vector3::new(
        f64::from(position.0.x) + 0.5,
        f64::from(position.0.y),
        f64::from(position.0.z) + 0.5,
    );
    let silverfish = from_type(&EntityType::SILVERFISH, pos, world, uuid::Uuid::new_v4());
    world.spawn_entity(silverfish.clone());
    world.send_entity_status(
        silverfish.get_entity(),
        EntityStatus::SilverfishMergeAnim,
        ActorEventID::None,
    );
}
