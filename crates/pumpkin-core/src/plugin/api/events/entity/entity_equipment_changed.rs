use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::EntityBase;

/// An entity's equipment changed (Paper `EntityEquipmentChangedEvent`).
#[derive(Event, Clone)]
pub struct EntityEquipmentChangedEvent {
    pub entity: Arc<dyn EntityBase>,
    pub slots: Vec<String>,
    pub new_items: Vec<ItemStack>,
}
