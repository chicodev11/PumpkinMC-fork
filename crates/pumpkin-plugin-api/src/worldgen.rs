use crate::wit::pumpkin::plugin::biomes::Biome;
use crate::wit::pumpkin::plugin::world::ChunkBuffer as WitChunkBuffer;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub use crate::wit::pumpkin::plugin::biomes::Biome as PluginBiome;
pub use crate::wit::pumpkin::plugin::world::GenerationPhase;

/// Wrapper around the WIT `chunk-buffer` resource representing a 16x16 chunk column
/// being generated.
pub struct ChunkBuffer {
    inner: WitChunkBuffer,
}

impl ChunkBuffer {
    /// Creates a new `ChunkBuffer` wrapper.
    #[must_use]
    pub const fn new(inner: WitChunkBuffer) -> Self {
        Self { inner }
    }

    /// Returns the chunk X coordinate.
    #[must_use]
    pub fn x(&self) -> i32 {
        self.inner.get_x()
    }

    /// Returns the chunk Z coordinate.
    #[must_use]
    pub fn z(&self) -> i32 {
        self.inner.get_z()
    }

    /// Returns the minimum Y coordinate of the world.
    #[must_use]
    pub fn min_y(&self) -> i32 {
        self.inner.get_min_y()
    }

    /// Returns the height of the chunk column in blocks.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.inner.get_height()
    }

    /// Sets the block state ID at local chunk coordinates `(x, y, z)` where `0 <= x < 16` and `0 <= z < 16`.
    pub fn set_block(&mut self, x: u8, y: i32, z: u8, state_id: u16) {
        self.inner.set_block_state_id(x, y, z, state_id);
    }

    /// Gets the block state ID at local chunk coordinates `(x, y, z)`.
    #[must_use]
    pub fn get_block(&self, x: u8, y: i32, z: u8) -> u16 {
        self.inner.get_block_state_id(x, y, z)
    }

    /// Fills an entire horizontal 16x16 layer at the given Y level with a block state ID.
    pub fn fill_layer(&mut self, y: i32, state_id: u16) {
        self.inner.fill_layer(y, state_id);
    }

    /// Fills a vertical column from `min_y` to `max_y` at local `(x, z)` with a block state ID.
    pub fn fill_range(&mut self, x: u8, min_y: i32, max_y: i32, z: u8, state_id: u16) {
        self.inner.fill_range(x, min_y, max_y, z, state_id);
    }

    /// Fills a 3D cuboid with a block state ID.
    pub fn fill_cuboid(
        &mut self,
        min_x: u8,
        min_y: i32,
        min_z: u8,
        max_x: u8,
        max_y: i32,
        max_z: u8,
        state_id: u16,
    ) {
        self.inner
            .fill_cuboid(min_x, min_y, min_z, max_x, max_y, max_z, state_id);
    }

    /// Sets the biome at local chunk coordinates `(x, y, z)`.
    pub fn set_biome(&mut self, x: u8, y: i32, z: u8, biome: Biome) {
        self.inner.set_biome(x, y, z, biome);
    }

    /// Fills the entire chunk column with a single biome.
    pub fn fill_biome(&mut self, biome: Biome) {
        self.inner.fill_biome(biome);
    }

    /// Like [`Self::set_biome`], by id: a vanilla biome (`"minecraft:plains"`) or one registered
    /// with `server.register-registry-entry("worldgen/biome", ...)` or a datapack. Fails for an
    /// unknown id.
    ///
    /// # Errors
    /// The biome id is unknown or past the 127 biomes a chunk can hold.
    pub fn set_biome_by_name(&mut self, x: u8, y: i32, z: u8, name: &str) -> Result<(), String> {
        self.inner.set_biome_by_name(x, y, z, name)
    }

    /// Like [`Self::fill_biome`], by id (see [`Self::set_biome_by_name`]).
    ///
    /// # Errors
    /// The biome id is unknown or past the 127 biomes a chunk can hold.
    pub fn fill_biome_by_name(&mut self, name: &str) -> Result<(), String> {
        self.inner.fill_biome_by_name(name)
    }

    /// Returns the world seed.
    #[must_use]
    pub fn seed(&self) -> i64 {
        self.inner.get_seed()
    }

    /// Replaces one 16x16x16 section. `section_y` is the section's block y >> 4; `states`
    /// holds 4096 block state IDs in y, z, x order (`(y * 16 + z) * 16 + x`). Returns false
    /// and changes nothing when the section is outside the world or `states` is not 4096 long.
    pub fn set_section(&mut self, section_y: i32, states: &[u16]) -> bool {
        self.inner.set_section(section_y, states)
    }

    /// Returns the 4096 block state IDs of one section in the [`Self::set_section`] order, or
    /// an empty list when the section is outside the world.
    #[must_use]
    pub fn get_section(&self, section_y: i32) -> Vec<u16> {
        self.inner.get_section(section_y)
    }

    /// Sets a block relative to this chunk's corner. In the features phase `x` and `z` may be
    /// -16..32 to reach the 8 neighbouring chunks (trees across chunk borders); otherwise only
    /// 0..16. Returns whether the block was written.
    pub fn set_block_relative(&mut self, x: i32, y: i32, z: i32, state_id: u16) -> bool {
        self.inner.set_block_relative(x, y, z, state_id)
    }

    /// Gets a block relative to this chunk's corner, with the reach of
    /// [`Self::set_block_relative`].
    #[must_use]
    pub fn get_block_relative(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        self.inner.get_block_relative(x, y, z)
    }
}

/// Trait for implementing custom world generation logic in plugins.
#[allow(unused_variables)]
pub trait ChunkGenerator: Send + Sync + 'static {
    /// Step 1: Assign biomes across the chunk column.
    fn generate_biomes(&self, chunk: &mut ChunkBuffer) {}

    /// Step 2: Generate basic terrain / noise shape into the chunk.
    fn generate_noise(&self, chunk: &mut ChunkBuffer) {}

    /// Step 3: Apply surface rules (e.g. grass, sand, stone layers).
    fn generate_surface(&self, chunk: &mut ChunkBuffer) {}

    /// Step 4: Populate chunk with features, structures, decorations, ores, etc.
    fn generate_features(&self, chunk: &mut ChunkBuffer) {}
}

pub(crate) static GENERATOR_HANDLERS: Mutex<BTreeMap<u32, Arc<dyn ChunkGenerator>>> =
    Mutex::new(BTreeMap::new());
static NEXT_GENERATOR_ID: Mutex<u32> = Mutex::new(0);

/// Manager for registering custom chunk generators with the server runtime.
pub struct GeneratorManager;

impl GeneratorManager {
    /// Registers a custom chunk generator and returns its unique generator ID.
    ///
    /// You can then set this generator on a world using `world.set_chunk_generator(id)`, use it
    /// for a new world (`WorldGenerator::Plugin(id)`), or offer it for the server's default
    /// worlds with `context.register_world_generator(name, settings)` in `on_load`, which the
    /// advanced config's `[world.generators]` then names as `"<plugin name>:<name>"`
    /// (`context.register_world_generator_with` takes any `WorldGenerator`, such as
    /// `NoiseWarped` or `NoiseWithPluginBiomes(id)`: vanilla terrain with this generator's
    /// biomes, for one world).
    pub fn register<G: ChunkGenerator>(generator: G) -> u32 {
        let mut id_lock = NEXT_GENERATOR_ID.lock().unwrap_or_else(|e| e.into_inner());
        let id = *id_lock;
        *id_lock += 1;

        let mut handlers = GENERATOR_HANDLERS.lock().unwrap_or_else(|e| e.into_inner());
        handlers.insert(id, Arc::new(generator));
        id
    }
}
