//! Structure-dependent mob spawn lists, vanilla `NaturalSpawner.mobsAt` and
//! `ChunkGenerator.getMobsAt`.
//!
//! Pumpkin does not keep structure starts on full chunks, so the starts that reference a
//! chunk are rebuilt from the seed (through the generator's structure start cache) and
//! remembered per chunk. A chunk load starts that in the background; a spawn check that
//! comes first computes it on the spot, so it always sees the same lists vanilla does.

use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use pumpkin_data::Block;
use pumpkin_data::biome::Spawner;
use pumpkin_data::entity::MobCategory;
use pumpkin_data::structures::{BoundingBoxType, Structure, StructureKeys};
use pumpkin_util::math::block_box::BlockBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::generation::generator::WorldGenerator;
use rustc_hash::{FxBuildHasher, FxHashMap};

use super::World;

/// Cached chunks before the cache is dropped and rebuilt on demand.
const MAX_CACHED_CHUNKS: usize = 16_384;

/// The parts of a structure start that spawn overrides test against.
pub struct StructureBounds {
    pub key: StructureKeys,
    /// `StructureStart.getBoundingBox`, including terrain adaptation padding.
    pub full: BlockBox,
    pub pieces: Box<[BlockBox]>,
}

impl StructureBounds {
    /// Vanilla `getStructureAt(..).isValid()` for `Full`, `structureHasPieceAt` for `Piece`.
    #[must_use]
    pub fn contains(&self, pos: &BlockPos, bounding_box: BoundingBoxType) -> bool {
        match bounding_box {
            BoundingBoxType::Full => self.full.contains_pos(&pos.0),
            BoundingBoxType::Piece => self.pieces.iter().any(|piece| piece.contains_pos(&pos.0)),
        }
    }
}

type ChunkStructures = Arc<[StructureBounds]>;

/// Per-world cache of the structures with spawn overrides that reference each chunk.
///
/// Rebuilding a start can mean generating the whole structure, so chunk loads queue it on the
/// rayon pool ahead of time ([`Self::structures_at`]); spawn checks wait for it
/// ([`Self::structures_at_blocking`]).
#[derive(Default)]
pub struct StructureSpawnCache {
    /// `None` while the chunk's structures are being computed.
    chunks: Arc<DashMap<(i32, i32), Option<ChunkStructures>, FxBuildHasher>>,
}

impl StructureSpawnCache {
    /// Structures with spawn overrides whose start references the chunk, or none yet while
    /// they are computed in the background.
    pub fn structures_at(&self, world: &World, chunk_x: i32, chunk_z: i32) -> ChunkStructures {
        let key = (chunk_x, chunk_z);
        if let Some(cached) = self.chunks.get(&key) {
            return cached.clone().unwrap_or_else(|| Arc::from([]));
        }
        if self.chunks.len() >= MAX_CACHED_CHUNKS {
            self.chunks.retain(|_, structures| structures.is_none());
        }
        match self.chunks.entry(key) {
            dashmap::Entry::Occupied(entry) => {
                return entry.get().clone().unwrap_or_else(|| Arc::from([]));
            }
            dashmap::Entry::Vacant(entry) => {
                entry.insert(None);
            }
        }
        let generator = world.level.world_gen.load_full();
        let chunks = self.chunks.clone();
        rayon::spawn(move || {
            let structures = compute_structures(&generator, chunk_x, chunk_z);
            chunks.insert(key, Some(structures));
        });
        Arc::from([])
    }

    /// [`Self::structures_at`] computed on the calling thread when missing.
    pub fn structures_at_blocking(
        &self,
        world: &World,
        chunk_x: i32,
        chunk_z: i32,
    ) -> ChunkStructures {
        if let Some(Some(cached)) = self.chunks.get(&(chunk_x, chunk_z)).as_deref() {
            return cached.clone();
        }
        let structures = compute_structures(&world.level.world_gen.load(), chunk_x, chunk_z);
        self.chunks
            .insert((chunk_x, chunk_z), Some(structures.clone()));
        structures
    }

    /// Vanilla `StructureManager.getStructureWithPieceAt` / `getStructureAt(..).isValid()`.
    pub fn is_inside(
        &self,
        world: &World,
        pos: &BlockPos,
        key: StructureKeys,
        bounding_box: BoundingBoxType,
    ) -> bool {
        self.structures_at_blocking(world, pos.0.x >> 4, pos.0.z >> 4)
            .iter()
            .any(|structure| structure.key == key && structure.contains(pos, bounding_box))
    }
}

fn compute_structures(generator: &WorldGenerator, chunk_x: i32, chunk_z: i32) -> ChunkStructures {
    match generator {
        WorldGenerator::Noise(generator) => generator
            .structure_references_at(chunk_x, chunk_z, &|key| {
                !Structure::get(&key).spawn_overrides.is_empty()
            })
            .into_iter()
            .filter_map(|(key, start)| {
                let mut collector = start
                    .collector
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                // StructureStart.isValid: a start without pieces does not count.
                if collector.is_empty() {
                    return None;
                }
                let full = collector.get_adjusted_bounding_box(&key);
                let pieces = collector
                    .pieces
                    .iter()
                    .map(|piece| piece.get_structure_piece().bounding_box)
                    .collect();
                Some(StructureBounds { key, full, pieces })
            })
            .collect(),
        WorldGenerator::Flat(_) | WorldGenerator::Custom(_) => Arc::from([]),
    }
}

/// Registry name of a mob category, as structure spawn overrides key them.
const fn category_name(category: &MobCategory) -> &'static str {
    match category.id {
        id if id == MobCategory::MONSTER.id => "monster",
        id if id == MobCategory::CREATURE.id => "creature",
        id if id == MobCategory::AMBIENT.id => "ambient",
        id if id == MobCategory::AXOLOTLS.id => "axolotls",
        id if id == MobCategory::UNDERGROUND_WATER_CREATURE.id => "underground_water_creature",
        id if id == MobCategory::WATER_CREATURE.id => "water_creature",
        id if id == MobCategory::WATER_AMBIENT.id => "water_ambient",
        _ => "misc",
    }
}

/// Override spawn lists converted to the biome spawner type, keyed by structure and category.
type OverrideSpawns = (BoundingBoxType, &'static [Spawner]);

static OVERRIDE_SPAWNS: LazyLock<FxHashMap<(StructureKeys, &'static str), OverrideSpawns>> =
    LazyLock::new(|| {
        let mut map = FxHashMap::default();
        for key in [
            StructureKeys::AncientCity,
            StructureKeys::Fortress,
            StructureKeys::Monument,
            StructureKeys::PillagerOutpost,
            StructureKeys::SwampHut,
            StructureKeys::TrialChambers,
        ] {
            for spawn_override in Structure::get(&key).spawn_overrides {
                let spawners: Vec<Spawner> = spawn_override
                    .spawns
                    .iter()
                    .map(|entry| Spawner {
                        r#type: entry.entity_type,
                        min_count: entry.min_count as i32,
                        max_count: entry.max_count as i32,
                        weight: entry.weight as i32,
                    })
                    .collect();
                map.insert(
                    (key, spawn_override.category),
                    (
                        spawn_override.bounding_box,
                        &*Box::leak(spawners.into_boxed_slice()),
                    ),
                );
            }
        }
        map
    });

/// Vanilla `NaturalSpawner.isInNetherFortressBounds`.
#[must_use]
pub fn is_in_nether_fortress_bounds(world: &World, pos: &BlockPos, category: &MobCategory) -> bool {
    category == &MobCategory::MONSTER
        && world.get_block_state_id(&pos.down()) == Block::NETHER_BRICKS.default_state.id
        && world.structure_spawns.is_inside(
            world,
            pos,
            StructureKeys::Fortress,
            BoundingBoxType::Full,
        )
}

/// Vanilla `NaturalSpawner.mobsAt` without the biome fallback: the spawn list a structure
/// imposes at `pos`, or `None` when the biome's own list applies.
#[must_use]
pub fn structure_mobs_at(
    world: &World,
    category: &MobCategory,
    pos: &BlockPos,
) -> Option<&'static [Spawner]> {
    let name = category_name(category);
    if is_in_nether_fortress_bounds(world, pos, category) {
        // NetherFortressStructure.FORTRESS_ENEMIES matches the fortress monster override.
        return OVERRIDE_SPAWNS
            .get(&(StructureKeys::Fortress, name))
            .map(|(_, spawns)| *spawns);
    }
    world
        .structure_spawns
        .structures_at_blocking(world, pos.0.x >> 4, pos.0.z >> 4)
        .iter()
        .find_map(|structure| {
            let (bounding_box, spawns) = OVERRIDE_SPAWNS.get(&(structure.key, name))?;
            structure.contains(pos, *bounding_box).then_some(*spawns)
        })
}

#[cfg(test)]
mod tests {
    use pumpkin_data::dimension::Dimension;

    use super::*;

    #[tokio::test]
    async fn fortress_pieces_replace_the_biome_monster_list()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = crate::entity::ai::test_support::world_in_dimension(
            &folder,
            Dimension::THE_NETHER.clone(),
        );
        let fortress = (-32..=32)
            .step_by(4)
            .flat_map(|x| (-32..=32).step_by(4).map(move |z| (x, z)))
            .find_map(|(x, z)| {
                world
                    .structure_spawns
                    .structures_at_blocking(&world, x, z)
                    .iter()
                    .find(|structure| structure.key == StructureKeys::Fortress)
                    .map(|structure| (structure.pieces.clone(), structure.full))
            });
        let (pieces, full) = fortress.ok_or("no fortress near the origin for seed 0")?;
        let piece = pieces.first().ok_or("fortress without pieces")?;
        let inside = BlockPos::new(piece.min.x, piece.min.y, piece.min.z);
        // Warm the asynchronous cache for the piece's chunk.
        world
            .structure_spawns
            .structures_at_blocking(&world, inside.0.x >> 4, inside.0.z >> 4);
        let spawns = structure_mobs_at(&world, &MobCategory::MONSTER, &inside)
            .ok_or("fortress piece kept the biome list")?;
        let wither = spawns
            .iter()
            .find(|spawner| spawner.r#type == "minecraft:wither_skeleton")
            .ok_or("no wither skeletons")?;
        assert_eq!(
            (wither.min_count, wither.max_count, wither.weight),
            (5, 5, 8)
        );
        // Fortress overrides only touch monsters.
        assert!(structure_mobs_at(&world, &MobCategory::AMBIENT, &inside).is_none());
        // Above the whole structure no piece contains the position.
        let above = BlockPos::new(piece.min.x, full.max.y + 1, piece.min.z);
        assert!(structure_mobs_at(&world, &MobCategory::MONSTER, &above).is_none());
        world.level.shutdown().await;
        Ok(())
    }
}
