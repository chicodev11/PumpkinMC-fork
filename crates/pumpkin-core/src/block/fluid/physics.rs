use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, fluid::Fluid, tag};
use pumpkin_data::{BlockDirection, BlockState};
use pumpkin_util::math::boundingbox::BoundingBox;

/// Check if a specific block can be replaced by fluid (based on block properties)
#[must_use]
pub fn can_be_replaced(block_state: &BlockState, block: &Block, fluid: &'static Fluid) -> bool {
    // Waterlogged blocks should not be replaced by water
    if block.is_waterlogged(block_state.id) {
        return false;
    }

    // Fluid Logic
    if let Some(other_fluid) = Fluid::from_state_id(block_state.id)
        && !fluid.matches_type(other_fluid)
    {
        return true;
    }

    let id = block.id;

    // Blocks that fluid should never replace
    if block.has_tag(&tag::Block::MINECRAFT_DOORS)
        || block.has_tag(&tag::Block::MINECRAFT_BEDS)
        || block.has_tag(&tag::Block::MINECRAFT_LEAVES)
        || block.has_tag(&tag::Block::MINECRAFT_PRESSURE_PLATES)
        || block.has_tag(&tag::Block::C_CLUSTERS)
        || block.has_tag(&tag::Block::MINECRAFT_WALL_CORALS)
        || block.has_tag(&tag::Block::MINECRAFT_SHULKER_BOXES)
        || block.has_tag(&tag::Block::MINECRAFT_PORTALS)
        || id == Block::BELL.id
        || id == Block::BIG_DRIPLEAF.id
        || id == Block::BIG_DRIPLEAF_STEM.id
        || id == Block::SMALL_DRIPLEAF.id
        || id == Block::CAKE.id
        || id == Block::CONDUIT.id
        || id == Block::CAMPFIRE.id
        || id == Block::DRAGON_EGG.id
        || id == Block::KELP.id
        || id == Block::KELP_PLANT.id
        || id == Block::SEAGRASS.id
        || id == Block::TALL_SEAGRASS.id
        || id == Block::LADDER.id
        || id == Block::POINTED_DRIPSTONE.id
        || id == Block::SCAFFOLDING.id
    {
        return false;
    }

    // Only replace air, explicitly replaceable blocks, or carpets
    block_state.replaceable()
        || id == Block::AIR.id
        || block.has_tag(&tag::Block::MINECRAFT_WOOL_CARPETS)
        // Only use PistonBehavior::Destroy if it didn't pass the checks above
        || block_state.piston_behavior == pumpkin_data::block_state::PistonBehavior::Destroy
}

const EPSILON: f64 = 1.0e-7;

fn is_full_block(shape: &BoundingBox) -> bool {
    shape.min.x <= EPSILON
        && shape.min.y <= EPSILON
        && shape.min.z <= EPSILON
        && shape.max.x >= 1.0 - EPSILON
        && shape.max.y >= 1.0 - EPSILON
        && shape.max.z >= 1.0 - EPSILON
}

/// Vanilla `FlowingFluid.canPassThroughWall`: fluid cannot move between two blocks when either
/// collision shape is a full block, or when their touching faces together cover the whole face.
#[must_use]
pub fn can_pass_through_wall(
    direction: BlockDirection,
    from: &BlockState,
    to: &BlockState,
) -> bool {
    if to.get_block_collision_shapes().any(|s| is_full_block(&s))
        || from.get_block_collision_shapes().any(|s| is_full_block(&s))
    {
        return false;
    }
    if to.collision_shapes.is_empty() && from.collision_shapes.is_empty() {
        return true;
    }
    !merged_face_occludes(from, to, direction)
}

/// `Shapes.mergedFaceOccludes`: `from`'s face towards `direction` plus `to`'s opposite face
/// cover the whole square (sampled on the 1/16 grid).
fn merged_face_occludes(from: &BlockState, to: &BlockState, direction: BlockDirection) -> bool {
    let touches = |shape: &BoundingBox, dir: BlockDirection| match dir {
        BlockDirection::Down => shape.min.y <= EPSILON,
        BlockDirection::Up => shape.max.y >= 1.0 - EPSILON,
        BlockDirection::North => shape.min.z <= EPSILON,
        BlockDirection::South => shape.max.z >= 1.0 - EPSILON,
        BlockDirection::West => shape.min.x <= EPSILON,
        BlockDirection::East => shape.max.x >= 1.0 - EPSILON,
    };
    let covers = |shape: &BoundingBox, u: f64, v: f64| match direction {
        BlockDirection::Down | BlockDirection::Up => {
            shape.min.x <= u && u <= shape.max.x && shape.min.z <= v && v <= shape.max.z
        }
        BlockDirection::North | BlockDirection::South => {
            shape.min.x <= u && u <= shape.max.x && shape.min.y <= v && v <= shape.max.y
        }
        BlockDirection::West | BlockDirection::East => {
            shape.min.z <= u && u <= shape.max.z && shape.min.y <= v && v <= shape.max.y
        }
    };
    let faces: Vec<BoundingBox> = from
        .get_block_collision_shapes()
        .filter(|s| touches(s, direction))
        .chain(
            to.get_block_collision_shapes()
                .filter(|s| touches(s, direction.opposite())),
        )
        .collect();
    if faces.is_empty() {
        return false;
    }
    (0..16).all(|i| {
        (0..16).all(|j| {
            let (u, v) = ((f64::from(i) + 0.5) / 16.0, (f64::from(j) + 0.5) / 16.0);
            faces.iter().any(|s| covers(s, u, v))
        })
    })
}
