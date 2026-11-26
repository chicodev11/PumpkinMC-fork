use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, types};
use super::nearest_living_entities::NearestLivingEntitySensor;
use super::{Sensor, is_entity_attackable};

const REQUIRES: [MemoryModuleId; 3] = [
    types::NEAREST_LIVING_ENTITIES.id(),
    types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
    types::NEAREST_ATTACKABLE.id(),
];

/// Vanilla `BreezeAttackEntitySensor`: the nearest living entity sensor, plus the first
/// attackable entity that isn't a creative or spectator player.
pub struct BreezeAttackEntitySensor(NearestLivingEntitySensor);

impl Default for BreezeAttackEntitySensor {
    fn default() -> Self {
        Self(NearestLivingEntitySensor)
    }
}

impl Sensor for BreezeAttackEntitySensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        self.0.do_tick(tick);
        let ctx = tick.visibility();
        let target = tick
            .state
            .get(types::NEAREST_LIVING_ENTITIES)
            .and_then(|entities| {
                entities
                    .iter()
                    .find(|entity| {
                        // Vanilla `EntitySelector.NO_CREATIVE_OR_SPECTATOR`.
                        !entity.is_spectator()
                            && !entity
                                .get_player()
                                .is_some_and(crate::entity::player::Player::is_creative)
                            && is_entity_attackable(&ctx, entity.as_ref())
                    })
                    .cloned()
            });
        tick.state.set_optional(types::NEAREST_ATTACKABLE, target);
    }
}
