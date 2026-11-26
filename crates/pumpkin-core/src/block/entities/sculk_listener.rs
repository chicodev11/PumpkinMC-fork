//! The `VibrationSystem.User` of sculk sensors, calibrated sculk sensors and shriekers.

use std::sync::{Arc, Mutex, Weak};

use pumpkin_data::block_properties::{
    CalibratedSculkSensorLikeProperties, SculkSensorPhase, SculkShriekerLikeProperties,
};
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::tag::{self, Tag};
use pumpkin_data::{Block, BlockId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::block::blocks::redstone::sculk_sensor::{
    SculkSensorBlock, get_calibrated_sculk_sensor_input_strength, sculk_sensor_phase,
};
use crate::block::blocks::sculk::sculk_shrieker::SculkShriekerBlock;
use crate::entity::EntityBase;
use crate::entity::vibration::{
    self, VibrationData, VibrationDestination, VibrationListener, VibrationUser,
    game_event_frequency, redstone_strength_for_distance,
};
use crate::world::World;
use crate::world::game_event::{GameEventContext, GameEventListener};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SculkListenerKind {
    Sensor,
    CalibratedSensor,
    Shrieker,
}

pub struct SculkListener {
    pos: BlockPos,
    kind: SculkListenerKind,
    vibration: Mutex<VibrationData>,
    world: Mutex<Weak<World>>,
}

impl SculkListener {
    #[must_use]
    pub fn new(pos: BlockPos, kind: SculkListenerKind) -> Self {
        Self {
            pos,
            kind,
            vibration: Mutex::new(VibrationData::default()),
            world: Mutex::new(Weak::new()),
        }
    }

    #[must_use]
    pub fn from_nbt(nbt: &NbtCompound, pos: BlockPos, kind: SculkListenerKind) -> Self {
        let listener = Self::new(pos, kind);
        if let Some(data) = nbt.get_compound("listener") {
            *listener
                .vibration
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = VibrationData::read_nbt(data);
        }
        listener
    }

    pub fn write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_compound(
            "listener",
            self.vibration
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .write_nbt(),
        );
    }

    /// Vanilla `VibrationSystem.Ticker.tick`, from the block entity's server tick.
    pub fn tick(&self, world: &Arc<World>) {
        *self
            .world
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::downgrade(world);
        vibration::tick(self);
    }

    pub fn visit(&self, visit: &mut dyn FnMut(&dyn GameEventListener)) {
        visit(&VibrationListener(self));
    }

    fn block(&self, world: &World) -> &'static Block {
        world.get_block(&self.pos)
    }

    /// Vanilla `SculkSensorBlock.canActivate`.
    fn sensor_can_activate(&self, world: &World) -> bool {
        let block = self.block(world);
        (block.id == BlockId::SCULK_SENSOR || block.id == BlockId::CALIBRATED_SCULK_SENSOR)
            && sculk_sensor_phase(block, world.get_block_state(&self.pos).id)
                == SculkSensorPhase::Inactive
    }
}

impl VibrationUser for SculkListener {
    fn vibration_data(&self) -> &Mutex<VibrationData> {
        &self.vibration
    }

    fn vibration_world(&self) -> Option<Arc<World>> {
        self.world
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .upgrade()
    }

    fn vibration_position(&self) -> Vector3<f64> {
        self.pos.to_centered_f64()
    }

    fn vibration_destination(&self) -> VibrationDestination {
        VibrationDestination::Block(self.pos)
    }

    fn vibration_listener_radius(&self) -> i32 {
        match self.kind {
            SculkListenerKind::CalibratedSensor => 16,
            SculkListenerKind::Sensor | SculkListenerKind::Shrieker => 8,
        }
    }

    fn listenable_events(&self) -> &'static Tag {
        match self.kind {
            SculkListenerKind::Shrieker => &tag::GameEvent::MINECRAFT_SHRIEKER_CAN_LISTEN,
            SculkListenerKind::Sensor | SculkListenerKind::CalibratedSensor => {
                &tag::GameEvent::MINECRAFT_VIBRATIONS
            }
        }
    }

    fn can_trigger_avoid_vibration(&self) -> bool {
        self.kind != SculkListenerKind::Shrieker
    }

    fn can_receive_vibration(
        &self,
        world: &World,
        pos: BlockPos,
        event: GameEvent,
        context: &GameEventContext<'_>,
    ) -> bool {
        match self.kind {
            SculkListenerKind::Shrieker => {
                let state = world.get_block_state(&self.pos);
                self.block(world).id == BlockId::SCULK_SHRIEKER
                    && !SculkShriekerLikeProperties::from_state_id(state.id).shrieking
                    && context.source_entity.is_some_and(|source| {
                        SculkShriekerBlock::try_get_player(world, source.get_entity()).is_some()
                    })
            }
            SculkListenerKind::Sensor | SculkListenerKind::CalibratedSensor => {
                if pos == self.pos
                    && (event == GameEvent::BlockDestroy || event == GameEvent::BlockPlace)
                {
                    return false;
                }
                if self.kind == SculkListenerKind::CalibratedSensor {
                    let state = world.get_block_state(&self.pos);
                    let facing =
                        CalibratedSculkSensorLikeProperties::from_state_id(state.id).facing;
                    let back = i32::from(get_calibrated_sculk_sensor_input_strength(
                        world, &self.pos, facing,
                    ));
                    if back != 0 && game_event_frequency(event) != back {
                        return false;
                    }
                }
                self.sensor_can_activate(world)
            }
        }
    }

    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        _pos: BlockPos,
        event: GameEvent,
        source: Option<Arc<dyn EntityBase>>,
        _projectile_owner: Option<Arc<dyn EntityBase>>,
        distance: f32,
    ) {
        // Bukkit `BlockReceiveGameEvent`: plugins may stop the block reacting.
        let mut received =
            crate::plugin::api::events::block::block_receive_game::BlockReceiveGameEvent {
                block_pos: self.pos,
                world: world.clone(),
                game_event: event.name().to_string(),
                source_entity: source.clone(),
                cancelled: false,
            };
        if world.fire_plugin_event(&mut received) {
            return;
        }
        match self.kind {
            SculkListenerKind::Shrieker => {
                if let Some(player) = source.and_then(|source| {
                    SculkShriekerBlock::try_get_player(world, source.get_entity())
                }) {
                    SculkShriekerBlock::try_shriek(world, &self.pos, &player);
                }
            }
            SculkListenerKind::Sensor | SculkListenerKind::CalibratedSensor => {
                if !self.sensor_can_activate(world) {
                    return;
                }
                let power =
                    redstone_strength_for_distance(distance, self.vibration_listener_radius());
                SculkSensorBlock::activate(
                    world,
                    &self.pos,
                    self.block(world),
                    power,
                    game_event_frequency(event),
                    source.as_deref(),
                );
            }
        }
    }
}
