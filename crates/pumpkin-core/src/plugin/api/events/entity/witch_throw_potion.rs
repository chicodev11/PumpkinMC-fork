use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;

/// A witch throws a potion (Paper `WitchThrowPotionEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct WitchThrowPotionEvent {
    pub witch: Arc<dyn EntityBase>,
    pub target: Arc<dyn EntityBase>,
    pub potion: Option<ItemStack>,
}
