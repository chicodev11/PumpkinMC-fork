use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::block::entities::hopper;
use crate::entity::{Entity, player::Player};
use pumpkin_data::translation;
use pumpkin_inventory::Inventory;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::text::TextComponent;

use super::container::{self, MinecartInventory};

#[derive(Clone)]
pub(super) struct HopperMinecart {
    enabled: Arc<AtomicBool>,
    inventory: Arc<MinecartInventory>,
}

impl HopperMinecart {
    pub(super) fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            inventory: Arc::new(MinecartInventory::new(5)),
        }
    }

    pub(super) const fn inventory(&self) -> &Arc<MinecartInventory> {
        &self.inventory
    }

    pub(super) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Vanilla `MinecartHopper.tryConsumeItems`: the block hopper's intake from above, then
    /// items touching the cart.
    pub(super) fn tick(&self, entity: &Entity) {
        if !self.enabled.load(Ordering::Relaxed) || !entity.is_alive() {
            return;
        }
        let world = entity.world.load_full();
        let pos = entity.pos.load();
        let inventory = self.inventory.as_ref();
        if hopper::suck_in_items(
            &world,
            inventory,
            Vector3::new(pos.x, pos.y + 0.5, pos.z),
            false,
            None,
        ) {
            inventory.mark_dirty();
            return;
        }
        let cart_box = entity.bounding_box.load().expand(0.25, 0.0, 0.25);
        for other in world.get_entities_at_box(&cart_box) {
            if other.get_entity().is_removed() {
                continue;
            }
            if let Some(item) = other.get_item_entity()
                && hopper::add_item_entity(inventory, item)
            {
                inventory.mark_dirty();
                return;
            }
        }
    }

    pub(super) fn interact(&self, entity: &crate::entity::Entity, player: &Arc<Player>) -> bool {
        container::open(
            entity,
            player,
            &self.inventory,
            TextComponent::translate_cross(
                translation::java::ENTITY_MINECRAFT_HOPPER_MINECART,
                translation::bedrock::ENTITY_HOPPER_MINECART_NAME,
                [],
            ),
            true,
        )
    }

    pub(super) fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.inventory.write_nbt(nbt);
        nbt.put_bool("Enabled", self.enabled.load(Ordering::Relaxed));
    }

    pub(super) fn read_nbt(&self, nbt: &NbtCompound) {
        self.inventory.read_nbt(nbt);
        self.enabled
            .store(nbt.get_bool("Enabled").unwrap_or(true), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::HopperMinecart;
    use pumpkin_inventory::Inventory;

    #[test]
    fn hopper_minecart_inventory_has_five_slots() {
        assert_eq!(HopperMinecart::new().inventory.size(), 5);
    }
}
