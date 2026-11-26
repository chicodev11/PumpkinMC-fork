use std::sync::Arc;

use pumpkin_data::translation;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::text::TextComponent;

use crate::entity::player::Player;

use super::container::{self, MinecartInventory};

#[derive(Clone)]
pub(super) struct ChestMinecart {
    inventory: Arc<MinecartInventory>,
}

impl ChestMinecart {
    pub(super) fn new() -> Self {
        Self {
            inventory: Arc::new(MinecartInventory::new(27)),
        }
    }

    pub(super) const fn inventory(&self) -> &Arc<MinecartInventory> {
        &self.inventory
    }

    /// Vanilla `MinecartChest.interact`: opening it is heard and angers piglins.
    pub(super) fn interact(&self, entity: &crate::entity::Entity, player: &Arc<Player>) -> bool {
        let opened = container::open(
            entity,
            player,
            &self.inventory,
            TextComponent::translate_cross(
                translation::java::ENTITY_MINECRAFT_CHEST_MINECART,
                translation::bedrock::ENTITY_CHEST_MINECART_NAME,
                [],
            ),
            false,
        );
        if opened {
            let world = entity.world.load();
            world.emit_game_event_with_context(
                pumpkin_data::game_event::GameEvent::ContainerOpen.name(),
                entity.pos.load(),
                &crate::world::game_event::GameEventContext {
                    source_entity: Some(player.as_ref()),
                },
            );
            crate::entity::mob::piglin_ai::anger_nearby_piglins(&world, player, true);
        }
        opened
    }

    pub(super) fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.inventory.write_nbt(nbt);
    }

    pub(super) fn read_nbt(&self, nbt: &NbtCompound) {
        self.inventory.read_nbt(nbt);
    }
}
