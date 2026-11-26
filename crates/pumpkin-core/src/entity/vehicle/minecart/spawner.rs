use pumpkin_data::entity::EntityStatus;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::math::position::BlockPos;

use crate::{
    block::entities::{BlockEntity, mob_spawner::MobSpawnerBlockEntity},
    entity::Entity,
};

/// Vanilla `MinecartSpawner`: a `BaseSpawner` (the same one the spawner block uses, with its
/// `SpawnData`, `SpawnPotentials`, `custom_spawn_rules` and `equipment`) ticked at the cart's
/// block position.
pub(super) struct SpawnerMinecart {
    spawner: std::sync::RwLock<MobSpawnerBlockEntity>,
}

impl SpawnerMinecart {
    pub(super) const fn new() -> Self {
        Self {
            spawner: std::sync::RwLock::new(MobSpawnerBlockEntity::new(
                BlockPos::new(0, 0, 0),
                None,
            )),
        }
    }

    pub(super) fn tick(&self, entity: &Entity) {
        let world = entity.world.load_full();
        let pos = entity.block_pos.load();
        let spawner = self
            .spawner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        spawner.tick_at(&world, pos, &|| {
            // Vanilla `MinecartSpawner.spawner.broadcastEvent`: entity event 1.
            world.broadcast_entity_event(entity, EntityStatus::Jump, ActorEventID::None);
        });
    }

    pub(super) fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.spawner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .write_spawner_nbt(nbt);
    }

    pub(super) fn read_nbt(&self, nbt: &NbtCompound) {
        *self
            .spawner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            MobSpawnerBlockEntity::from_nbt(nbt, BlockPos::new(0, 0, 0));
    }
}
