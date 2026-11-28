use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A witch readies a potion (Paper `WitchReadyPotionEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct WitchReadyPotionEvent {
    pub witch: Arc<dyn EntityBase>,
    pub potion: Option<ItemStack>,
}
