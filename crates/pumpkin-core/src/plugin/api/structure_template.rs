//! Plugin structure templates: vanilla `.nbt` files (`StructureTemplate`) registered by
//! plugins, placed in a world or listed block by block for plugin generators.

use std::sync::Arc;

use pumpkin_data::{BlockState, BlockStateId, Rotation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::generation::structure::template::{
    BlockPlacer, StructureTemplate, get_template, global_cache, place_template,
};

use crate::world::World;
use crate::world::block_placer::WorldBlockPlacer;

/// Registers (or replaces) template `name` from the bytes of a vanilla structure `.nbt` file
/// (gzip-compressed or not).
pub fn register(name: &str, bytes: Vec<u8>) -> Result<(), String> {
    if !name.contains(':') || name.starts_with("minecraft:") {
        return Err(format!(
            "template name `{name}` needs the plugin's own namespace"
        ));
    }
    StructureTemplate::from_nbt_bytes(&bytes)
        .map_err(|error| format!("not a structure template: {error}"))?;
    global_cache().register_template(name, Arc::from(bytes));
    Ok(())
}

fn template(name: &str) -> Result<Arc<StructureTemplate>, String> {
    get_template(name).ok_or_else(|| format!("unknown structure template `{name}`"))
}

/// Places template `name` with its minimum corner at `origin`, like `/place template`
/// (structure voids keep the world's block; block entities come from the template).
pub fn place(
    world: &World,
    name: &str,
    origin: Vector3<i32>,
    rotation: Rotation,
    skip_air: bool,
) -> Result<(), String> {
    let template = template(name)?;
    let mut placer = WorldBlockPlacer::new(world);
    place_template(
        &mut placer,
        &template,
        origin,
        (0, 0),
        rotation,
        skip_air,
        false,
        &[],
        None,
    );
    placer.finalize();
    world.queue_block_updates(&placer.changed_positions);
    world.flush_block_updates();
    Ok(())
}

#[derive(Default)]
struct Collector {
    blocks: Vec<(Vector3<i32>, BlockStateId)>,
}

impl BlockPlacer for Collector {
    fn get_block_state(&self, _pos: &Vector3<i32>) -> BlockStateId {
        pumpkin_data::Block::AIR.default_state.id
    }

    fn set_block_state(&mut self, pos: &Vector3<i32>, state: &BlockState) {
        self.blocks.push((*pos, state.id));
    }

    fn add_block_entity(&mut self, _nbt: NbtCompound) {}
}

/// The blocks template `name` places, relative to its minimum corner, after `rotation`:
/// for plugin generators that place it chunk by chunk. Structure voids are left out.
pub fn blocks(
    name: &str,
    rotation: Rotation,
    skip_air: bool,
) -> Result<Vec<(Vector3<i32>, BlockStateId)>, String> {
    let template = template(name)?;
    let mut collector = Collector::default();
    place_template(
        &mut collector,
        &template,
        Vector3::new(0, 0, 0),
        (0, 0),
        rotation,
        skip_air,
        false,
        &[],
        None,
    );
    Ok(collector.blocks)
}
