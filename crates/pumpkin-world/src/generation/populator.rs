//! Chunk populators (Paper `BlockPopulator`).
//!
//! Plugin code that runs on a chunk right after a vanilla generation stage (surface or
//! features), for noise and flat worlds. Custom generators have their own phases. While none is
//! registered this is a single atomic load per chunk.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use crate::chunk_system::generation_cache::Cache;

/// The vanilla stage a populator runs after.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopulatorStage {
    /// After the surface rules, before carvers and features.
    Surface,
    /// After the vanilla features and structures.
    Features,
}

/// Runs on the center chunk of `cache`; may place blocks (vanilla or custom states). In the
/// features stage the cache also holds the chunk's neighbours, which may be written too.
pub trait ChunkPopulator: Send + Sync {
    fn populate(&self, stage: PopulatorStage, cache: &mut Cache);
}

/// Runs on a chunk right after the vanilla biome stage (before noise) and may replace its
/// biomes; set on one world's generator (`VanillaGenerator::biome_populator`).
pub trait BiomePopulator: Send + Sync {
    fn populate_biomes(&self, chunk: &mut crate::generation::proto_chunk::ProtoChunk);
}

struct Entry {
    dimension: &'static str,
    stage: PopulatorStage,
    populator: Arc<dyn ChunkPopulator>,
}

static POPULATORS: RwLock<Vec<Entry>> = RwLock::new(Vec::new());
static COUNT: AtomicUsize = AtomicUsize::new(0);

/// Adds a features-stage populator for worlds of `dimension` (`minecraft:overworld`, ...).
pub fn register(dimension: &'static str, populator: Arc<dyn ChunkPopulator>) {
    register_at(PopulatorStage::Features, dimension, populator);
}

/// Adds a populator for worlds of `dimension` that runs after `stage`.
pub fn register_at(
    stage: PopulatorStage,
    dimension: &'static str,
    populator: Arc<dyn ChunkPopulator>,
) {
    let mut populators = POPULATORS.write().unwrap_or_else(PoisonError::into_inner);
    populators.push(Entry {
        dimension,
        stage,
        populator,
    });
    COUNT.store(populators.len(), Ordering::Release);
}

/// Runs every populator registered for `dimension` and `stage` on the center chunk of `cache`.
pub fn run(dimension: &str, stage: PopulatorStage, cache: &mut Cache) {
    if COUNT.load(Ordering::Acquire) == 0 {
        return;
    }
    let populators: Vec<Arc<dyn ChunkPopulator>> = POPULATORS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|entry| entry.dimension == dimension && entry.stage == stage)
        .map(|entry| entry.populator.clone())
        .collect();
    for populator in populators {
        populator.populate(stage, cache);
    }
}
