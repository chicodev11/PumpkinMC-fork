use std::sync::Arc;

use pumpkin_data::enchantment::LevelBasedValue;
use pumpkin_util::math::vector3::Vector3;

use super::EnchantmentEntityEffectExt;
use crate::entity::player::Player;
use crate::entity::{Entity, EntityBase};
use crate::world::World;

/// Enchantment entity effect that sets an entity on fire for a duration calculated from the level.
#[derive(Clone, Debug, PartialEq)]
pub struct Ignite {
    pub duration: LevelBasedValue,
}

impl Ignite {
    #[must_use]
    pub const fn new(duration: LevelBasedValue) -> Self {
        Self { duration }
    }

    /// Applies the ignite effect to an entity for the given enchantment level.
    pub fn apply_to_entity(&self, level: i32, entity: &Entity) {
        self.apply_from(level, entity, None);
    }

    /// Fire Aspect and similar: `attacker` is reported to `EntityCombustByEntityEvent`.
    pub fn apply_from(&self, level: i32, entity: &Entity, attacker: Option<i32>) {
        let mut seconds = self.duration.calculate(level);
        if let Some(attacker) = attacker {
            let Some(allowed) = crate::entity::combust_by_entity(entity, seconds, attacker) else {
                return;
            };
            seconds = allowed;
        }
        entity.set_on_fire_for_cause(
            seconds,
            crate::plugin::api::events::entity::entity_combust::CombustCause::Attack,
        );
        entity.set_on_fire(true);
    }
}

impl EnchantmentEntityEffectExt for Ignite {
    fn apply(
        &self,
        _world: &Arc<World>,
        enchantment_level: i32,
        owner: Option<&Arc<Player>>,
        entity: Option<&Entity>,
        _position: Vector3<f64>,
    ) {
        if let Some(entity) = entity {
            self.apply_from(
                enchantment_level,
                entity,
                owner.map(|owner| owner.entity_id()),
            );
        }
    }
}
