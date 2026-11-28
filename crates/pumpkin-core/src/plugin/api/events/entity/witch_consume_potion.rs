use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A witch drinks a potion (Paper `WitchConsumePotionEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct WitchConsumePotionEvent {
    pub witch: Arc<dyn EntityBase>,
    pub potion: Option<ItemStack>,
}
