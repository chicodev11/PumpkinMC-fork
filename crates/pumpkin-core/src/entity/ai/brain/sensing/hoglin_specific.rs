use pumpkin_data::entity::EntityType;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::position::BlockPos;

use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;
use crate::entity::living::LivingEntity;
use crate::entity::mob::hoglin::HoglinEntity;

const REQUIRES: [MemoryModuleId; 6] = [
    types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
    types::NEAREST_REPELLENT.id(),
    types::NEAREST_VISIBLE_ADULT_PIGLIN.id(),
    types::NEAREST_VISIBLE_ADULT_HOGLINS.id(),
    types::VISIBLE_ADULT_PIGLIN_COUNT.id(),
    types::VISIBLE_ADULT_HOGLIN_COUNT.id(),
];

/// Vanilla `HoglinSpecificSensor`: the nearest hoglin repellent within 8 blocks (4 up or down)
/// and the visible adult piglins and hoglins, which decide whether hoglins hold or retreat.
pub struct HoglinSpecificSensor;

impl Sensor for HoglinSpecificSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        let origin = tick.mob.get_entity().block_pos.load();
        let repellent = BlockPos::iterate_outwards(origin, 8, 4, 8).find(|pos| {
            pumpkin_data::Block::from_state_id(tick.world.get_block_state_id(pos))
                .has_tag(&tag::Block::MINECRAFT_HOGLIN_REPELLENTS)
        });
        tick.state.set_optional(types::NEAREST_REPELLENT, repellent);
        if let Some(hoglin) = tick.mob.cast_any().downcast_ref::<HoglinEntity>() {
            hoglin.nearest_repellent.store(repellent);
        }

        let mut nearest_piglin = None;
        let mut piglin_count = 0;
        let mut hoglins = Vec::new();
        if let Some(visible) = tick.state.get(types::NEAREST_VISIBLE_LIVING_ENTITIES) {
            let ctx = tick.visibility();
            // Vanilla matches `Piglin` and `Hoglin` exactly: brutes and zoglins do not count.
            for entity in visible.find_all(&ctx, |entity| {
                let entity_type = entity.get_entity().entity_type;
                (entity_type == &EntityType::PIGLIN || entity_type == &EntityType::HOGLIN)
                    && !entity
                        .get_living_entity()
                        .is_some_and(LivingEntity::is_baby)
            }) {
                if entity.get_entity().entity_type == &EntityType::PIGLIN {
                    piglin_count += 1;
                    if nearest_piglin.is_none() {
                        nearest_piglin = Some(entity.clone());
                    }
                } else {
                    hoglins.push(entity.clone());
                }
            }
        }
        let hoglin_count = i32::try_from(hoglins.len()).unwrap_or(i32::MAX);
        tick.state
            .set_optional(types::NEAREST_VISIBLE_ADULT_PIGLIN, nearest_piglin);
        tick.state
            .set(types::NEAREST_VISIBLE_ADULT_HOGLINS, hoglins);
        tick.state
            .set(types::VISIBLE_ADULT_PIGLIN_COUNT, piglin_count);
        tick.state
            .set(types::VISIBLE_ADULT_HOGLIN_COUNT, hoglin_count);
    }
}
