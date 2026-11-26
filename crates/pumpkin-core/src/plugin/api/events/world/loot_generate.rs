use pumpkin_macros::{Event, cancellable};

/// An event that occurs when loot is generated.
#[cancellable]
#[derive(Event, Clone)]
pub struct LootGenerateEvent {
    /// Loot table identifier.
    pub loot_table: String,
    /// Bukkit `getLoot`: the generated stacks; plugins may change the list.
    pub items: Vec<pumpkin_data::item_stack::ItemStack>,
}

impl LootGenerateEvent {
    #[must_use]
    pub const fn new(loot_table: String) -> Self {
        Self::with_items(loot_table, Vec::new())
    }

    #[must_use]
    pub const fn with_items(
        loot_table: String,
        items: Vec<pumpkin_data::item_stack::ItemStack>,
    ) -> Self {
        Self {
            loot_table,
            items,
            cancelled: false,
        }
    }
}
