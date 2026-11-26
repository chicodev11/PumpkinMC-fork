//! Vanilla `PlayerSpawnFinder` (where a player without a respawn point appears) and
//! `MinecraftServer.setInitialSpawn` (where a new world puts its spawn).

use pumpkin_data::BlockDirection;
use pumpkin_util::GameMode;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::chunk::ChunkHeightmapType;

use super::World;

/// Vanilla `PlayerSpawnFinder.ABSOLUTE_MAX_ATTEMPTS`.
const ABSOLUTE_MAX_ATTEMPTS: i64 = 1024;
/// Player bounding box size (`EntityType.PLAYER` dimensions).
const PLAYER_WIDTH: f64 = 0.6;
const PLAYER_HEIGHT: f64 = 1.8;

/// Vanilla `getCoprime`: a step through all candidates in a scattered order.
const fn coprime(candidates: i32) -> i32 {
    if candidates <= 16 { candidates - 1 } else { 17 }
}

/// The `respawn_radius` square around the suggestion, cut down to the world border.
fn spawn_radius(world: &World, suggestion: BlockPos) -> i32 {
    let rule = world.level_info.load().game_rules.respawn_radius;
    let mut radius = i32::try_from(rule.max(0)).unwrap_or(i32::MAX);
    let to_border = world
        .worldborder
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_distance_to_border(f64::from(suggestion.0.x), f64::from(suggestion.0.z))
        .floor() as i32;
    if to_border < radius {
        radius = to_border;
    }
    if to_border <= 1 {
        radius = 1;
    }
    radius
}

/// Vanilla `PlayerSpawnFinder.findSpawn`.
///
/// A random free spot in the `respawn_radius` square around the world spawn, else the
/// suggestion moved to the nearest free height. Adventure worlds spawn at the suggestion itself.
pub async fn find_player_spawn(world: &World, suggestion: BlockPos) -> Vector3<f64> {
    let adventure = world.server.upgrade().is_some_and(|server| {
        server
            .defaultgamemode
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .gamemode
            == GameMode::Adventure
    });
    if adventure {
        load_chunk(world, suggestion.0.x, suggestion.0.z).await;
        return fixup_spawn_height(world, suggestion);
    }
    let radius = spawn_radius(world, suggestion);
    let side = i64::from(radius) * 2 + 1;
    let candidates = i32::try_from(ABSOLUTE_MAX_ATTEMPTS.min(side * side)).unwrap_or(1024);
    let step = coprime(candidates);
    let offset = rand::random_range(0..candidates);
    let width = radius * 2 + 1;
    for index in 0..candidates {
        let candidate = (offset + step * index) % candidates;
        let x = suggestion.0.x + candidate % width - radius;
        let z = suggestion.0.z + candidate / width - radius;
        load_chunk(world, x, z).await;
        if let Some(pos) = level_respawn_pos(world, x, z)
            && no_collision_no_liquid(world, pos)
        {
            return bottom_center(pos);
        }
    }
    load_chunk(world, suggestion.0.x, suggestion.0.z).await;
    fixup_spawn_height(world, suggestion)
}

async fn load_chunk(world: &World, x: i32, z: i32) {
    world
        .level
        .get_or_fetch_chunk(Vector2::new(x >> 4, z >> 4), |_| ())
        .await;
}

fn bottom_center(pos: BlockPos) -> Vector3<f64> {
    Vector3::new(
        f64::from(pos.0.x) + 0.5,
        f64::from(pos.0.y),
        f64::from(pos.0.z) + 0.5,
    )
}

/// Vanilla `fixupSpawnHeight`: up out of anything solid or wet, then down onto the ground.
fn fixup_spawn_height(world: &World, suggestion: BlockPos) -> Vector3<f64> {
    let top = world.dimension.min_y + world.dimension.height;
    let bottom = world.dimension.min_y;
    let mut pos = suggestion;
    while !no_collision_no_liquid(world, pos) && pos.0.y < top {
        pos = pos.up();
    }
    pos = pos.down();
    while no_collision_no_liquid(world, pos) && pos.0.y > bottom {
        pos = pos.down();
    }
    bottom_center(pos.up())
}

/// Vanilla `noCollisionNoLiquid`: a player box standing at `pos` touches no block collision
/// and no fluid (entities are not checked: the search has no entity).
fn no_collision_no_liquid(world: &World, pos: BlockPos) -> bool {
    let base = bottom_center(pos);
    let half = PLAYER_WIDTH / 2.0;
    let bounding_box = BoundingBox::new(
        Vector3::new(base.x - half, base.y, base.z - half),
        Vector3::new(base.x + half, base.y + PLAYER_HEIGHT, base.z + half),
    );
    if !world.is_space_empty(bounding_box) {
        return false;
    }
    BlockPos::iterate(bounding_box.min_block_pos(), bounding_box.max_block_pos())
        .all(|block_pos| world.get_fluid(&block_pos).id == pumpkin_data::fluid::Fluid::EMPTY.id)
}

/// Vanilla `Heightmap.getHeight`: the first free Y above the column's top block.
fn height(world: &World, heightmap: ChunkHeightmapType, x: i32, z: i32) -> i32 {
    let min_y = world.dimension.min_y;
    world
        .level
        .read_chunk_sync(&Vector2::new(x >> 4, z >> 4), |chunk| {
            chunk
                .heightmap
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(heightmap, x, z, min_y)
                + 1
        })
        .unwrap_or(min_y)
}

/// Vanilla `OCEAN_FLOOR` height: above the top block that blocks motion, fluids ignored.
fn ocean_floor_height(world: &World, x: i32, z: i32, surface: i32) -> i32 {
    let min_y = world.dimension.min_y;
    let mut y = surface - 1;
    while y >= min_y {
        let pos = BlockPos::new(x, y, z);
        let state = world.get_block_state(&pos);
        if pumpkin_data::block_properties::blocks_movement(state, state.id.to_block_id()) {
            return y + 1;
        }
        y -= 1;
    }
    min_y
}

/// Vanilla `PlayerSpawnFinder.getLevelRespawnPos`.
///
/// The spot above the highest full-topped block of a dry column, `None` for water columns,
/// holes to the void or flooded caves. The column's chunk must be loaded.
#[must_use]
pub fn level_respawn_pos(world: &World, x: i32, z: i32) -> Option<BlockPos> {
    let min_y = world.dimension.min_y;
    let top_y = if world.dimension.has_ceiling {
        world.sea_level
    } else {
        height(world, ChunkHeightmapType::MotionBlocking, x, z)
    };
    if top_y < min_y {
        return None;
    }
    let surface = height(world, ChunkHeightmapType::WorldSurface, x, z);
    if surface <= top_y && surface > ocean_floor_height(world, x, z, surface) {
        return None;
    }
    let mut y = top_y + 1;
    while y >= min_y {
        let pos = BlockPos::new(x, y, z);
        if world.get_fluid(&pos).id != pumpkin_data::fluid::Fluid::EMPTY.id {
            return None;
        }
        let state = world.get_block_state(&pos);
        // Vanilla `Block.isFaceFull(collisionShape, UP)`.
        if state.is_side_solid(BlockDirection::Up) {
            return Some(pos.up());
        }
        y -= 1;
    }
    None
}

/// Vanilla `PlayerSpawnFinder.getSpawnPosInChunk`: the first column with a respawn spot.
#[must_use]
pub fn spawn_pos_in_chunk(world: &World, chunk_x: i32, chunk_z: i32) -> Option<BlockPos> {
    for x in chunk_x * 16..chunk_x * 16 + 16 {
        for z in chunk_z * 16..chunk_z * 16 + 16 {
            if let Some(pos) = level_respawn_pos(world, x, z) {
                return Some(pos);
            }
        }
    }
    None
}

/// Vanilla `MinecraftServer.setInitialSpawn`: from the generator's spawn chunk, spiral out
/// over an 11x11 chunk square to the first chunk with a respawn spot.
pub async fn initial_spawn(world: &World, origin_chunk: Vector2<i32>) -> BlockPos {
    let origin_x = origin_chunk.x * 16;
    let origin_z = origin_chunk.y * 16;
    load_chunk(world, origin_x + 8, origin_z + 8).await;
    // Vanilla `NoiseBasedChunkGenerator.getSpawnHeight` is the sea level; below the world it
    // falls back to the surface.
    let mut spawn_y = world.sea_level;
    if spawn_y < world.dimension.min_y {
        spawn_y = height(
            world,
            ChunkHeightmapType::WorldSurface,
            origin_x + 8,
            origin_z + 8,
        );
    }
    let mut spawn = BlockPos::new(origin_x + 8, spawn_y, origin_z + 8);
    let (mut dx, mut dz, mut step_x, mut step_z) = (0i32, 0i32, 0i32, -1i32);
    for _ in 0..11 * 11 {
        if (-5..=5).contains(&dx) && (-5..=5).contains(&dz) {
            let chunk_x = origin_chunk.x + dx;
            let chunk_z = origin_chunk.y + dz;
            load_chunk(world, chunk_x * 16, chunk_z * 16).await;
            if let Some(pos) = spawn_pos_in_chunk(world, chunk_x, chunk_z) {
                spawn = pos;
                break;
            }
        }
        if dx == dz || (dx < 0 && dx == -dz) || (dx > 0 && dx == 1 - dz) {
            let turned = step_x;
            step_x = -step_z;
            step_z = turned;
        }
        dx += step_x;
        dz += step_z;
    }
    spawn
}

#[cfg(test)]
mod tests {
    use super::coprime;

    #[test]
    fn coprime_steps_visit_every_candidate() {
        for candidates in [1, 9, 16, 121, 1024] {
            let step = coprime(candidates);
            let mut seen = vec![false; candidates as usize];
            for index in 0..candidates {
                seen[((3 + step * index) % candidates) as usize] = true;
            }
            assert!(seen.iter().all(|s| *s), "{candidates}");
        }
    }
}
