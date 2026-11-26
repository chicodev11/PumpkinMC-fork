use crate::entity::player::Player;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

/// How a drag spreads the cursor stack (Bukkit `DragType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragType {
    /// Left button: the stack is split evenly over the slots.
    Even,
    /// Right button: one item per slot.
    Single,
    /// Middle button (creative): a full stack per slot.
    Clone,
}

/// Fired once when a player finishes dragging the cursor stack over slots (Bukkit
/// `InventoryDragEvent`). Cancelling it leaves the slots and the cursor unchanged.
#[cancellable]
#[derive(Event, Clone)]
pub struct InventoryDragEvent {
    /// The player dragging items.
    pub player: Arc<Player>,
    /// The slots the drag went over, in order.
    pub raw_slots: Vec<i16>,
    /// How the stack is spread.
    pub drag_type: DragType,
    /// The cursor stack before the drag is applied.
    pub old_cursor: ItemStack,
}

impl InventoryDragEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        raw_slots: Vec<i16>,
        drag_type: DragType,
        old_cursor: ItemStack,
    ) -> Self {
        Self {
            player,
            raw_slots,
            drag_type,
            old_cursor,
            cancelled: false,
        }
    }
}
