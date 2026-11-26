use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;
use super::nearest_living_entities::NearestLivingEntitySensor;
use crate::entity::mob::warden::WardenEntity;

const REQUIRES: [MemoryModuleId; 3] = [
    types::NEAREST_LIVING_ENTITIES.id(),
    types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
    types::NEAREST_ATTACKABLE.id(),
];

/// Vanilla `WardenEntitySensor`: the nearest living entity sensor, plus the closest entity the
/// warden can target, players first.
pub struct WardenEntitySensor(NearestLivingEntitySensor);

impl Default for WardenEntitySensor {
    fn default() -> Self {
        Self(NearestLivingEntitySensor)
    }
}

impl Sensor for WardenEntitySensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        self.0.do_tick(tick);
        let Some(warden) = tick.mob.cast_any().downcast_ref::<WardenEntity>() else {
            return;
        };
        let target = tick
            .state
            .get(types::NEAREST_LIVING_ENTITIES)
            .and_then(|entities| {
                let closest = |players: bool| {
                    entities
                        .iter()
                        .find(|entity| {
                            entity.get_player().is_some() == players
                                && warden.can_target_entity(entity.as_ref())
                        })
                        .cloned()
                };
                closest(true).or_else(|| closest(false))
            });
        tick.state.set_optional(types::NEAREST_ATTACKABLE, target);
    }
}
