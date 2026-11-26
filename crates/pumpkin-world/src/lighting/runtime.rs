use crate::chunk::io::Dirtiable;
use crate::chunk::palette::BlockPalette;
use crate::level::Level;
use crate::lighting::engine::{occluding_faces, shape_occludes};
use crossbeam::queue::SegQueue;
use pumpkin_config::lighting::LightingEngineConfig;
use pumpkin_data::BlockDirection;
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

/// Vanilla sky light step into a block of `opacity` moving `dir`: full sky light falls
/// straight down through blocks that let it pass, anything else loses `max(1, opacity)`.
const fn sky_light_into(light: u8, opacity: u8, dir: BlockDirection) -> u8 {
    if light == 15 && opacity == 0 && matches!(dir, BlockDirection::Down) {
        15
    } else {
        light.saturating_sub(if opacity > 1 { opacity } else { 1 })
    }
}

pub struct DynamicLightEngine {
    min_y: i32,
    max_y: i32,
    block_decrease: SegQueue<(BlockPos, u8)>,
    block_increase: SegQueue<(BlockPos, u8)>,
    sky_decrease: SegQueue<(BlockPos, u8)>,
    sky_increase: SegQueue<(BlockPos, u8)>,
}

impl DynamicLightEngine {
    #[must_use]
    pub const fn new(min_y: i32, max_y: i32) -> Self {
        Self {
            min_y,
            max_y,
            block_decrease: SegQueue::new(),
            block_increase: SegQueue::new(),
            sky_decrease: SegQueue::new(),
            sky_increase: SegQueue::new(),
        }
    }
}

impl DynamicLightEngine {
    /// Checks if there is an open sky above the given position (no opaque blocks blocking sky light).
    fn has_open_sky_above(&self, level: &Arc<Level>, pos: &BlockPos) -> bool {
        let max_y = self.max_y - 1;
        let mut current_pos = *pos;
        let mut below_faces = occluding_faces(level.get_block_state(pos).to_state());

        // Scan upward until we hit sky or a block that stops straight-down sky light.
        while current_pos.0.y < max_y {
            current_pos.0.y += 1;

            let state = level.get_block_state(&current_pos).to_state();
            let faces = occluding_faces(state);
            if state.opacity > 0 || shape_occludes(faces, below_faces, BlockDirection::Down) {
                return false;
            }
            below_faces = faces;
        }

        true // Reached sky without hitting opaque blocks
    }

    /// Handles all lighting updates triggered by a block change (placement/break).
    /// This updates Block Light, Sky Light, and ensures the source block is valid.
    pub fn update_lighting_at(&self, level: &Arc<Level>, pos: BlockPos) {
        // Block Light
        self.check_block_light_updates(level, pos);
        self.perform_block_light_updates(level);

        // Sky Light
        self.check_sky_light_updates(level, pos);
        self.perform_sky_light_updates(level);
    }

    pub fn queue_block_light_decrease(&self, pos: BlockPos, level: u8) {
        self.block_decrease.push((pos, level));
    }

    pub fn queue_block_light_increase(&self, pos: BlockPos, level: u8) {
        self.block_increase.push((pos, level));
    }

    pub fn queue_sky_light_decrease(&self, pos: BlockPos, level: u8) {
        self.sky_decrease.push((pos, level));
    }

    pub fn queue_sky_light_increase(&self, pos: BlockPos, level: u8) {
        self.sky_increase.push((pos, level));
    }

    pub fn perform_block_light_updates(&self, level: &Arc<Level>) -> i32 {
        let mut updates = 0;

        // Keep processing until both queues are empty
        // Light propagation queues new updates, so we need to process until convergence
        loop {
            let decrease_updates = self.perform_block_light_decrease_updates(level);
            let increase_updates = self.perform_block_light_increase_updates(level);

            updates += decrease_updates + increase_updates;

            // Stop when no more updates were processed
            if decrease_updates == 0 && increase_updates == 0 {
                break;
            }
        }

        updates
    }

    fn perform_block_light_decrease_updates(&self, level: &Arc<Level>) -> i32 {
        let mut updates = 0;

        while let Some((pos, expected_light)) = self.block_decrease.pop() {
            self.propagate_block_light_decrease(level, &pos, expected_light);
            updates += 1;
        }

        updates
    }

    fn perform_block_light_increase_updates(&self, level: &Arc<Level>) -> i32 {
        let mut updates = 0;

        while let Some((pos, expected_light)) = self.block_increase.pop() {
            self.propagate_block_light_increase(level, &pos, expected_light);
            updates += 1;
        }

        updates
    }

    fn propagate_block_light_increase(&self, level: &Arc<Level>, pos: &BlockPos, light_level: u8) {
        for dir in BlockDirection::all() {
            let neighbor_pos = pos.offset(dir.to_offset());

            if neighbor_pos.0.y < self.min_y || neighbor_pos.0.y >= self.max_y {
                continue;
            }

            if let Some(neighbor_light) = self.get_block_light_level(level, &neighbor_pos) {
                let neighbor_state = level.get_block_state(&neighbor_pos).to_state();
                let opacity = neighbor_state.opacity.max(1);
                let new_light = light_level.saturating_sub(opacity);

                // Only propagate if new light is brighter than current light
                if new_light > neighbor_light
                    && self
                        .set_block_light_level(level, &neighbor_pos, new_light)
                        .is_ok()
                    && new_light > 1
                {
                    self.queue_block_light_increase(neighbor_pos, new_light);
                }
            }
        }
    }

    fn propagate_block_light_decrease(
        &self,
        level: &Arc<Level>,
        pos: &BlockPos,
        removed_light_level: u8,
    ) {
        // Check what the current light level actually is at this position
        let current_level = self.get_block_light_level(level, pos).unwrap_or(0);

        // Only propagate decrease if this position hasn't already been reset to 0
        // This prevents positions that were intentionally set to 0 from propagating light
        if current_level == 0 && removed_light_level > 0 {
            // This position was already darkened, so we propagate the darkness to neighbors
            for dir in BlockDirection::all() {
                let neighbor_pos = pos.offset(dir.to_offset());

                if neighbor_pos.0.y < self.min_y || neighbor_pos.0.y >= self.max_y {
                    continue;
                }

                if let Some(neighbor_light) = self.get_block_light_level(level, &neighbor_pos) {
                    if neighbor_light == 0 {
                        continue; // Skip if already 0
                    }

                    let neighbor_state = level.get_block_state(&neighbor_pos).to_state();
                    let opacity = neighbor_state.opacity.max(1);

                    let expected_from_removed_source = removed_light_level.saturating_sub(opacity);

                    if neighbor_light <= expected_from_removed_source {
                        let neighbor_luminance = neighbor_state.luminance;

                        if neighbor_luminance == 0 {
                            // No self-emission, darken it completely and continue propagation
                            self.set_block_light_level(level, &neighbor_pos, 0).ok();
                            self.queue_block_light_decrease(neighbor_pos, neighbor_light);
                        } else {
                            // Has self-emission, set to its own light and re-propagate from it
                            self.set_block_light_level(level, &neighbor_pos, neighbor_luminance)
                                .ok();
                            self.queue_block_light_increase(neighbor_pos, neighbor_luminance);
                        }
                    } else {
                        // This neighbor has brighter light from another source, re-propagate from it
                        self.queue_block_light_increase(neighbor_pos, neighbor_light);
                    }
                }
            }
        }
    }

    pub fn check_block_light_updates(&self, level: &Arc<Level>, pos: BlockPos) {
        match level.lighting_config {
            LightingEngineConfig::Full => {
                self.set_block_light_level(level, &pos, 15).ok();
                return;
            }
            LightingEngineConfig::Dark => {
                self.set_block_light_level(level, &pos, 0).ok();
                return;
            }
            LightingEngineConfig::Default => {}
        }

        let current_light = self.get_block_light_level(level, &pos).unwrap_or(0);
        let block_state = level.get_block_state(&pos).to_state();
        let expected_light = block_state.luminance;

        // Handle light decrease (removing light source or placing opaque block)
        if expected_light < current_light {
            // Set to expected value immediately, then queue decrease to darken neighbors
            self.set_block_light_level(level, &pos, expected_light).ok();
            self.queue_block_light_decrease(pos, current_light);
        } else if expected_light > current_light {
            // Handle light increase (placing light source)
            self.set_block_light_level(level, &pos, expected_light).ok();
            self.queue_block_light_increase(pos, expected_light);
        }

        // Only check neighbors if we didn't trigger a decrease
        // Decrease propagation handles re-validating neighbors
        if expected_light >= current_light {
            self.check_neighbors_light_updates(level, pos, expected_light);
        }
    }

    pub fn check_neighbors_light_updates(
        &self,
        level: &Arc<Level>,
        pos: BlockPos,
        current_light: u8,
    ) {
        for dir in BlockDirection::all() {
            let neighbor_pos = pos.offset(dir.to_offset());

            if neighbor_pos.0.y < self.min_y || neighbor_pos.0.y >= self.max_y {
                continue;
            }
            if let Some(neighbor_light) = self.get_block_light_level(level, &neighbor_pos)
                && neighbor_light > current_light + 1
            {
                self.queue_block_light_increase(neighbor_pos, neighbor_light);
            }
        }
    }

    pub fn perform_sky_light_updates(&self, level: &Arc<Level>) -> i32 {
        let mut updates = 0;
        loop {
            let decrease_updates = self.perform_sky_light_decrease_updates(level);
            let increase_updates = self.perform_sky_light_increase_updates(level);

            updates += decrease_updates + increase_updates;

            if decrease_updates == 0 && increase_updates == 0 {
                break;
            }
        }
        updates
    }

    fn perform_sky_light_decrease_updates(&self, level: &Arc<Level>) -> i32 {
        let mut updates = 0;
        while let Some((pos, expected_light)) = self.sky_decrease.pop() {
            self.propagate_sky_light_decrease(level, &pos, expected_light);
            updates += 1;
        }
        updates
    }

    fn perform_sky_light_increase_updates(&self, level: &Arc<Level>) -> i32 {
        let mut updates = 0;
        while let Some((pos, expected_light)) = self.sky_increase.pop() {
            self.propagate_sky_light_increase(level, &pos, expected_light);
            updates += 1;
        }
        updates
    }

    fn propagate_sky_light_increase(&self, level: &Arc<Level>, pos: &BlockPos, light_level: u8) {
        let from_faces = occluding_faces(level.get_block_state(pos).to_state());
        for dir in BlockDirection::all() {
            let neighbor_pos = pos.offset(dir.to_offset());

            if neighbor_pos.0.y < self.min_y || neighbor_pos.0.y >= self.max_y {
                continue;
            }

            // Never propagate into an unloaded chunk. Writes to an unloaded
            // chunk are dropped silently, so the "brighter than neighbor" check
            // below would stay true forever and keep re-queuing the same
            // position, spinning this loop indefinitely at the border between
            // loaded and unloaded chunks.
            let (neighbor_chunk, _) = neighbor_pos.chunk_and_chunk_relative_position();
            if !level.is_chunk_loaded(&neighbor_chunk) {
                continue;
            }

            let neighbor_light = self.get_sky_light_level(level, &neighbor_pos);
            let neighbor_state = level.get_block_state(&neighbor_pos).to_state();
            if shape_occludes(from_faces, occluding_faces(neighbor_state), dir) {
                continue;
            }
            let new_light = sky_light_into(light_level, neighbor_state.opacity, dir);

            // Only propagate if new light is brighter than current light
            if new_light > neighbor_light {
                self.set_sky_light_level(level, &neighbor_pos, new_light)
                    .ok();

                if new_light > 0 {
                    self.queue_sky_light_increase(neighbor_pos, new_light);
                }
            }
        }
    }

    fn propagate_sky_light_decrease(&self, level: &Arc<Level>, pos: &BlockPos, removed_light: u8) {
        let from_faces = occluding_faces(level.get_block_state(pos).to_state());
        for dir in BlockDirection::all() {
            let neighbor_pos = pos.offset(dir.to_offset());

            if neighbor_pos.0.y < self.min_y || neighbor_pos.0.y >= self.max_y {
                continue;
            }

            // See `propagate_sky_light_increase`: skip unloaded chunks so sky
            // light updates never spin at loaded/unloaded chunk borders.
            let (neighbor_chunk, _) = neighbor_pos.chunk_and_chunk_relative_position();
            if !level.is_chunk_loaded(&neighbor_chunk) {
                continue;
            }

            let neighbor_light = self.get_sky_light_level(level, &neighbor_pos);
            if neighbor_light == 0 {
                continue; // Already dark
            }

            let neighbor_state = level.get_block_state(&neighbor_pos).to_state();
            if shape_occludes(from_faces, occluding_faces(neighbor_state), dir) {
                continue;
            }
            // What we would have given this neighbor
            let expected = sky_light_into(removed_light, neighbor_state.opacity, dir);

            if neighbor_light == expected || neighbor_light < removed_light {
                // This neighbor was lit by us, darken it
                self.set_sky_light_level(level, &neighbor_pos, 0).ok();
                self.queue_sky_light_decrease(neighbor_pos, neighbor_light);
            } else {
                // Lit by another source, such as an open-sky column beside a new roof that is
                // as bright as the removed light: re-propagate from it to fill the gap.
                self.queue_sky_light_increase(neighbor_pos, neighbor_light);
            }
        }
    }

    pub fn check_sky_light_updates(&self, level: &Arc<Level>, pos: BlockPos) {
        match level.lighting_config {
            LightingEngineConfig::Full => {
                self.set_sky_light_level(level, &pos, 15).ok();
                return;
            }
            LightingEngineConfig::Dark => {
                self.set_sky_light_level(level, &pos, 0).ok();
                return;
            }
            LightingEngineConfig::Default => {}
        }

        if !level.world_gen.load().dimension().has_skylight {
            return;
        }

        let current_light = self.get_sky_light_level(level, &pos);
        let block_state = level.get_block_state(&pos).to_state();
        let opacity = block_state.opacity;

        // Calculate expected sky light
        let expected_light = if opacity == 15 || block_state.is_solid_render() {
            // Fully opaque block = no light
            0
        } else if self.has_open_sky_above(level, &pos) {
            sky_light_into(15, opacity, BlockDirection::Down)
        } else {
            // No direct sky: the best light any neighbor sends in.
            let faces = occluding_faces(block_state);
            let mut best_light = 0;
            for dir in BlockDirection::all() {
                let neighbor_pos = pos.offset(dir.to_offset());

                if neighbor_pos.0.y < self.min_y || neighbor_pos.0.y >= self.max_y {
                    continue;
                }

                let neighbor_faces =
                    occluding_faces(level.get_block_state(&neighbor_pos).to_state());
                if shape_occludes(neighbor_faces, faces, dir.opposite()) {
                    continue;
                }
                let neighbor_light = self.get_sky_light_level(level, &neighbor_pos);
                best_light =
                    best_light.max(sky_light_into(neighbor_light, opacity, dir.opposite()));
            }
            best_light
        };

        // Update if needed
        if expected_light < current_light {
            // Light decreased
            self.set_sky_light_level(level, &pos, expected_light).ok();
            self.queue_sky_light_decrease(pos, current_light);
        } else if expected_light > current_light {
            // Light increased
            self.set_sky_light_level(level, &pos, expected_light).ok();
            self.queue_sky_light_increase(pos, expected_light);
        }

        // Notify neighbors if light increased or stayed same
        if expected_light >= current_light {
            self.check_neighbors_sky_light_updates(pos, expected_light);
        }
    }

    pub fn check_neighbors_sky_light_updates(&self, pos: BlockPos, current_light: u8) {
        // When we update a position, propagate to neighbors
        if current_light > 0 {
            self.queue_sky_light_increase(pos, current_light);
        }
    }

    pub fn get_block_light_level_sync(&self, level: &Level, position: &BlockPos) -> Option<u8> {
        let (chunk_pos, relative) = position.chunk_and_chunk_relative_position();

        level.read_chunk_sync(&chunk_pos, |chunk| {
            let section_idx = (relative.y - chunk.section.min_y) as usize / 16;
            let light_engine = chunk.light_engine.lock().ok()?;

            light_engine
                .block_light
                .get(section_idx)?
                .get(
                    relative.x as usize,
                    (relative.y - chunk.section.min_y) as usize % 16,
                    relative.z as usize,
                )
                .into()
        })?
    }

    pub fn get_sky_light_level_sync(&self, level: &Level, position: &BlockPos) -> u8 {
        let (chunk_coordinate, relative) = position.chunk_and_chunk_relative_position();
        level
            .read_chunk_sync(&chunk_coordinate, |chunk| {
                let section_index =
                    (relative.y - chunk.section.min_y) as usize / BlockPalette::SIZE;

                let light_engine = chunk
                    .light_engine
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                // Bounds check for section index (lock the light engine)
                if section_index >= light_engine.sky_light.len() {
                    return 15;
                }

                light_engine.sky_light[section_index].get(
                    relative.x as usize,
                    (relative.y - chunk.section.min_y) as usize % BlockPalette::SIZE,
                    relative.z as usize,
                )
            })
            .unwrap_or(0)
    }

    pub fn get_block_light_level(&self, level: &Arc<Level>, position: &BlockPos) -> Option<u8> {
        let (chunk_pos, relative) = position.chunk_and_chunk_relative_position();

        level
            .read_chunk_sync(&chunk_pos, |chunk| {
                let section_idx = (relative.y - chunk.section.min_y) as usize / 16;
                chunk
                    .light_engine
                    .lock()
                    .ok()?
                    .block_light
                    .get(section_idx)
                    .map(|section| {
                        section.get(
                            relative.x as usize,
                            (relative.y - chunk.section.min_y) as usize % 16,
                            relative.z as usize,
                        )
                    })
            })
            .flatten()
    }

    pub fn set_block_light_level(
        &self,
        level: &Arc<Level>,
        position: &BlockPos,
        light_level: u8,
    ) -> Result<(), String> {
        let (chunk_coordinate, relative) = position.chunk_and_chunk_relative_position();
        level.read_chunk_sync(&chunk_coordinate, |chunk| {
            let section_index = (relative.y - chunk.section.min_y) as usize / BlockPalette::SIZE;
            {
                let mut light_engine = chunk
                    .light_engine
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if section_index >= light_engine.block_light.len() {
                    return Err("Invalid section index".to_string());
                }
                let relative_y = (relative.y - chunk.section.min_y) as usize % BlockPalette::SIZE;
                light_engine.block_light[section_index].set(
                    relative.x as usize,
                    relative_y,
                    relative.z as usize,
                    light_level,
                );
            };
            // Mark chunk as dirty so lighting changes are saved to disk
            if !chunk.is_dirty() {
                chunk.mark_dirty(true);
            }
            Ok(())
        });
        Ok(())
    }

    pub fn get_sky_light_level(&self, level: &Arc<Level>, position: &BlockPos) -> u8 {
        let (chunk_coordinate, relative) = position.chunk_and_chunk_relative_position();
        level
            .read_chunk_sync(&chunk_coordinate, |chunk| {
                let section_index =
                    (relative.y - chunk.section.min_y) as usize / BlockPalette::SIZE;

                let light_engine = chunk
                    .light_engine
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                // Bounds check for section index (lock the light engine)
                if section_index >= light_engine.sky_light.len() {
                    return 15;
                }

                light_engine.sky_light[section_index].get(
                    relative.x as usize,
                    (relative.y - chunk.section.min_y) as usize % BlockPalette::SIZE,
                    relative.z as usize,
                )
            })
            .unwrap_or(0)
    }

    pub fn set_sky_light_level(
        &self,
        level: &Arc<Level>,
        position: &BlockPos,
        light_level: u8,
    ) -> Result<(), String> {
        let (chunk_coordinate, relative) = position.chunk_and_chunk_relative_position();
        level.read_chunk_sync(&chunk_coordinate, |chunk| {
            let section_index = (relative.y - chunk.section.min_y) as usize / BlockPalette::SIZE;
            {
                let mut light_engine = chunk
                    .light_engine
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if section_index >= light_engine.sky_light.len() {
                    return Err("Invalid section index".to_string());
                }
                let relative_y = (relative.y - chunk.section.min_y) as usize % BlockPalette::SIZE;
                light_engine.sky_light[section_index].set(
                    relative.x as usize,
                    relative_y,
                    relative.z as usize,
                    light_level,
                );
            };
            // Mark chunk as dirty so lighting changes are saved to disk
            if !chunk.is_dirty() {
                chunk.mark_dirty(true);
            }
            Ok(())
        });
        Ok(())
    }
}
