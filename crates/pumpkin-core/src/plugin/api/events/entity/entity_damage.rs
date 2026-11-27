use std::sync::Arc;

use pumpkin_data::damage::DamageType;
use pumpkin_macros::{Event, cancellable};

use crate::entity::EntityBase;
use crate::world::World;

/// An event that occurs when an entity takes damage.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityDamageEvent {
    /// The ID of the entity taking damage.
    pub entity_id: i32,

    /// The amount of damage taken.
    pub damage: f32,

    /// The damage type.
    pub damage_type: DamageType,

    /// Bukkit `getEntity`: the entity taking damage.
    pub entity: Option<Arc<dyn EntityBase>>,

    /// Paper `getDamageSource().getDirectEntity()`: what hit it (an arrow, or the attacker).
    pub direct_entity_id: Option<i32>,

    /// Paper `getDamageSource().getCausingEntity()`: who is responsible (the shooter).
    pub causing_entity_id: Option<i32>,
}

impl EntityDamageEvent {
    #[must_use]
    pub const fn new(entity_id: i32, damage_type: DamageType, damage_amount: f32) -> Self {
        Self {
            entity_id,
            damage: damage_amount,
            damage_type,
            entity: None,
            direct_entity_id: None,
            causing_entity_id: None,
            cancelled: false,
        }
    }

    /// Adds the damaged entity (looked up in `world`) and Paper's damage source entities.
    #[must_use]
    pub fn with_source(
        mut self,
        world: &World,
        direct: Option<&dyn EntityBase>,
        causing: Option<&dyn EntityBase>,
    ) -> Self {
        self.entity = world.get_entity_by_id(self.entity_id);
        self.direct_entity_id = direct.map(|entity| entity.get_entity().entity_id);
        self.causing_entity_id = causing.map(|entity| entity.get_entity().entity_id);
        self
    }
}
