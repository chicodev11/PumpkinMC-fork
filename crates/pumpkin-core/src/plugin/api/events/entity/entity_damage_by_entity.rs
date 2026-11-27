use pumpkin_data::damage::DamageType;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};

use crate::entity::EntityBase;

/// An event that occurs when an entity takes damage from another entity (Bukkit
/// `EntityDamageByEntityEvent` with its `DamageSource`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityDamageByEntityEvent {
    /// The ID of the entity taking damage.
    pub entity_id: i32,

    /// The ID of the entity that hit directly (`getDamager`): the attacker for a melee hit, the
    /// arrow or other projectile for a ranged one.
    pub damager_id: i32,

    /// The ID of the entity responsible (`DamageSource.getCausingEntity`): the shooter of a
    /// projectile, the attacker of a melee hit. `None` for an ownerless projectile.
    pub causing_entity_id: Option<i32>,

    /// The amount of damage dealt.
    pub damage: f32,

    /// The damage type.
    pub damage_type: DamageType,

    /// The damage cause (the damage type's debug name, kept for older plugins).
    pub cause: String,

    /// The weapon used (`DamageSource.getWeaponItem`): the main hand item of the causing
    /// living entity (for a projectile, what it was shot with is still in that hand), if any.
    pub weapon: Option<ItemStack>,

    /// Bukkit `getEntity`: the entity taking damage.
    pub entity: Option<std::sync::Arc<dyn EntityBase>>,
}

impl EntityDamageByEntityEvent {
    /// Builds the event for a hit on `entity_id` by `direct`, caused by `causing`.
    #[must_use]
    pub fn new(
        entity_id: i32,
        direct: &dyn EntityBase,
        causing: Option<&dyn EntityBase>,
        damage: f32,
        damage_type: DamageType,
    ) -> Self {
        let weapon = causing.and_then(|causing| {
            causing
                .get_living_entity()
                .map(|living| living.held_item(causing))
                .filter(|stack| !stack.is_empty())
        });
        Self {
            entity_id,
            damager_id: direct.get_entity().entity_id,
            causing_entity_id: causing.map(|causing| causing.get_entity().entity_id),
            damage,
            damage_type,
            cause: format!("{damage_type:?}"),
            weapon,
            entity: None,
            cancelled: false,
        }
    }

    /// Adds the damaged entity, looked up in `world`.
    #[must_use]
    pub fn with_entity(mut self, world: &crate::world::World) -> Self {
        self.entity = world.get_entity_by_id(self.entity_id);
        self
    }
}
