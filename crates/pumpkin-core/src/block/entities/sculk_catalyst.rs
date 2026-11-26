use super::BlockEntity;
use crate::block::blocks::sculk::spreader::SculkSpreader;
use crate::entity::player::advancement::trigger::AdvancementTrigger;
use crate::world::World;
use crate::world::game_event::{GameEventContext, GameEventListener};
use pumpkin_data::block_properties::SculkCatalystLikeProperties;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::{Block, BlockId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

/// Vanilla `SculkCatalystBlockEntity`: eats the experience of nearby deaths and spreads sculk.
pub struct SculkCatalystBlockEntity {
    pub position: BlockPos,
    spreader: Mutex<SculkSpreader>,
}

/// Vanilla `SculkCatalystBlockEntity.CatalystListener`.
struct CatalystListener<'a>(&'a SculkCatalystBlockEntity);

impl GameEventListener for CatalystListener<'_> {
    fn listener_position(&self) -> Vector3<f64> {
        self.0.position.to_centered_f64()
    }

    fn listener_radius(&self) -> i32 {
        8
    }

    fn handle_game_event(
        &self,
        _world: &World,
        event: GameEvent,
        context: &GameEventContext<'_>,
        source_position: Vector3<f64>,
    ) -> bool {
        if event != GameEvent::EntityDie {
            return false;
        }
        let Some(source) = context.source_entity else {
            return false;
        };
        let Some(living) = source.get_living_entity() else {
            return false;
        };
        if !living.experience_consumed.swap(true, Ordering::Relaxed) {
            let experience = source.get_experience_reward(None);
            if source.should_drop_experience() && experience > 0 {
                let cursor = BlockPos::floored_v(source_position + Vector3::new(0.0, 0.5, 0.0));
                // Bukkit `SculkBloomEvent`: plugins may cancel or change the charge.
                let world = source.get_entity().world.load_full();
                let mut bloom = crate::plugin::api::events::block::sculk_bloom::SculkBloomEvent {
                    block_pos: cursor,
                    world: world.clone(),
                    charge: i32::try_from(experience).unwrap_or(i32::MAX),
                    cancelled: false,
                };
                if world.fire_plugin_event(&mut bloom) {
                    return true;
                }
                self.0
                    .spreader
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .add_cursors(cursor, bloom.charge.max(0));
                // Vanilla `tryAwardItSpreadsAdvancement`: the killer earns "It Spreads".
                if let Some(killer) = living.get_kill_credit()
                    && let Some(player) = killer.get_player()
                {
                    player.trigger_advancement(AdvancementTrigger::KillMobNearSculkCatalyst);
                }
            }
        }
        // Vanilla blooms right away; the dying entity's world is this catalyst's world.
        self.0.bloom(&source.get_entity().world.load_full());
        true
    }

    fn delivers_by_distance(&self) -> bool {
        true
    }
}

impl SculkCatalystBlockEntity {
    pub const ID: &'static str = "minecraft:sculk_catalyst";

    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            spreader: Mutex::new(SculkSpreader::default()),
        }
    }

    /// Vanilla `CatalystListener.bloom`.
    fn bloom(&self, world: &Arc<World>) {
        let pos = self.position;
        let block = world.get_block(&pos);
        if block.id != BlockId::SCULK_CATALYST {
            return;
        }
        let mut props = SculkCatalystLikeProperties::from_state_id(world.get_block_state_id(&pos));
        props.bloom = true;
        world.set_block_state(&pos, props.to_state_id(block), BlockFlags::NOTIFY_ALL);
        world.schedule_block_tick(&Block::SCULK_CATALYST, pos, 8, TickPriority::Normal);
        world.spawn_particle(
            Vector3::new(
                f64::from(pos.0.x) + 0.5,
                f64::from(pos.0.y) + 1.15,
                f64::from(pos.0.z) + 0.5,
            ),
            Vector3::new(0.2, 0.0, 0.2),
            0.0,
            2,
            Particle::SculkSoul,
        );
        let pitch = rand::rng().random::<f32>().mul_add(0.4, 0.6);
        world.play_sound_fine(
            Sound::BlockSculkCatalystBloom,
            SoundCategory::Blocks,
            &pos.to_centered_f64(),
            2.0,
            pitch,
        );
    }
}

impl BlockEntity for SculkCatalystBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let entity = Self::new(position);
        entity
            .spreader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .read_nbt(nbt);
        entity
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.spreader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .write_nbt(nbt);
    }

    /// Vanilla `SculkCatalystBlockEntity.serverTick`.
    fn tick(&self, world: &Arc<World>) {
        let mut spreader = self
            .spreader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        spreader.update_cursors(world, &self.position, &mut rand::rng());
    }

    fn for_each_game_event_listener(&self, visit: &mut dyn FnMut(&dyn GameEventListener)) {
        visit(&CatalystListener(self));
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
