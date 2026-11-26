use super::BlockEntity;
use super::sculk_listener::{SculkListener, SculkListenerKind};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use std::sync::Mutex;

pub struct CalibratedSculkSensorBlockEntity {
    pub position: BlockPos,
    pub listener: SculkListener,
    pub last_vibration_frequency: Mutex<i32>,
}

impl BlockEntity for CalibratedSculkSensorBlockEntity {
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
        let last_vibration_frequency = nbt.get_int("last_vibration_frequency").unwrap_or(0);
        Self {
            position,
            listener: SculkListener::from_nbt(nbt, position, SculkListenerKind::CalibratedSensor),
            last_vibration_frequency: Mutex::new(last_vibration_frequency),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.listener.write_nbt(nbt);
        if let Ok(freq) = self.last_vibration_frequency.lock() {
            nbt.put_int("last_vibration_frequency", *freq);
        }
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        nbt.put_int(
            "last_vibration_frequency",
            *self.last_vibration_frequency.try_lock().ok()?,
        );
        Some(nbt)
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

impl CalibratedSculkSensorBlockEntity {
    pub const ID: &'static str = "minecraft:calibrated_sculk_sensor";
    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            listener: SculkListener::new(position, SculkListenerKind::CalibratedSensor),
            last_vibration_frequency: Mutex::new(0),
        }
    }
}
