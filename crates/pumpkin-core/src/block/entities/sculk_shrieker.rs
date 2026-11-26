use super::BlockEntity;
use super::sculk_listener::{SculkListener, SculkListenerKind};
use crate::block::blocks::sculk::sculk_shrieker::SculkShriekerBlock;
use crate::world::World;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct SculkShriekerBlockEntity {
    pub position: BlockPos,
    pub listener: SculkListener,
    pub warning_level: Mutex<i32>,
    pub shrieking_can_summon: AtomicBool,
}

impl BlockEntity for SculkShriekerBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let warning_level = nbt.get_int("warning_level").unwrap_or(0);
        Self {
            position,
            listener: SculkListener::from_nbt(nbt, position, SculkListenerKind::Shrieker),
            warning_level: Mutex::new(warning_level),
            shrieking_can_summon: AtomicBool::new(false),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.listener.write_nbt(nbt);
        if let Ok(warning_level) = self.warning_level.lock() {
            nbt.put_int("warning_level", *warning_level);
        }
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        nbt.put_int("warning_level", *self.warning_level.try_lock().ok()?);
        Some(nbt)
    }

    fn on_block_replaced(self: Arc<Self>, world: &Arc<World>, position: &BlockPos) {
        if self.shrieking_can_summon.swap(false, Ordering::Relaxed) {
            let warning_level = *self
                .warning_level
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            SculkShriekerBlock::respond(world, position, warning_level);
        }
    }

    fn tick(&self, world: &std::sync::Arc<crate::world::World>) {
        self.listener.tick(world);
    }

    fn for_each_game_event_listener(
        &self,
        visit: &mut dyn FnMut(&dyn crate::world::game_event::GameEventListener),
    ) {
        self.listener.visit(visit);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SculkShriekerBlockEntity {
    pub const ID: &'static str = "minecraft:sculk_shrieker";
    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            listener: SculkListener::new(position, SculkListenerKind::Shrieker),
            warning_level: Mutex::new(0),
            shrieking_can_summon: AtomicBool::new(false),
        }
    }
}
