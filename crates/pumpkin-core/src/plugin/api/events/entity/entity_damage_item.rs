use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A non-player entity's item loses durability (Paper `EntityDamageItemEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityDamageItemEvent {
    pub entity: Arc<dyn EntityBase>,
    pub item: ItemStack,
    pub damage: i32,
}
