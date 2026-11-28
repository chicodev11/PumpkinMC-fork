use crate::block::entities::BlockEntity;
use crate::entity::EntityBase;
use crate::entity::ai::brain::memory::types::HEARD_BELL_TIME;
use crate::world::World;
use crossbeam::atomic::AtomicCell;
use pumpkin_data::block_properties::HorizontalFacing;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{BlockDirection, HorizontalFacingExt};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use std::any::Any;
use std::sync::Arc;

pub struct BellBlockEntity {
    pub position: BlockPos,
    pub last_side_hit: AtomicCell<Option<HorizontalFacing>>,
    pub ring_ticks: AtomicCell<i32>,
    pub ringing: AtomicCell<bool>,
    resonating: AtomicCell<bool>,
    resonate_time: AtomicCell<i32>,
    /// Vanilla `lastRingTimestamp` / `nearbyEntities`: living entities within 48 blocks, looked
    /// up again at most every 60 ticks.
    last_ring_timestamp: AtomicCell<i64>,
    nearby_entities: std::sync::Mutex<Option<Vec<std::sync::Weak<dyn EntityBase>>>>,
}

impl BellBlockEntity {
    pub const ID: &'static str = "minecraft:bell";
    #[must_use]
    pub const fn new(position: BlockPos) -> Self {
        Self {
            position,
            last_side_hit: AtomicCell::new(None),
            ring_ticks: AtomicCell::new(0),
            resonate_time: AtomicCell::new(0),
            resonating: AtomicCell::new(false),
            ringing: AtomicCell::new(false),
            last_ring_timestamp: AtomicCell::new(0),
            nearby_entities: std::sync::Mutex::new(None),
        }
    }
    /// Vanilla `onHit`: shakes the bell and sends block event 1 with the hit side, whose
    /// `triggerEvent` tells the nearby entities.
    pub fn activate(&self, world: &World, direction: HorizontalFacing) {
        self.last_side_hit.store(Some(direction));
        if self.ringing.load() {
            self.ring_ticks.store(0);
        } else {
            self.ringing.store(true);
        }
        world.add_synced_block_event(
            self.position,
            Self::RING_EVENT,
            direction.to_block_direction().to_index(),
        );
    }

    pub const RING_EVENT: u8 = 1;

    /// Vanilla `triggerEvent(1, direction)`.
    pub fn trigger_event(&self, world: &World, data: u8) {
        self.update_entities(world);
        self.resonate_time.store(0);
        if let Some(direction) =
            BlockDirection::from_index(data).and_then(|d| d.to_horizontal_facing())
        {
            self.last_side_hit.store(Some(direction));
        }
        self.ring_ticks.store(0);
        self.ringing.store(true);
    }

    /// Vanilla `updateEntities`: villagers (and any brain with `HEARD_BELL_TIME`) within 32
    /// blocks remember the ring.
    fn update_entities(&self, world: &World) {
        let game_time = world.get_world_age();
        let mut nearby = self
            .nearby_entities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if game_time > self.last_ring_timestamp.load() + 60 || nearby.is_none() {
            self.last_ring_timestamp.store(game_time);
            let area = BoundingBox::from_block(&self.position).expand(48.0, 48.0, 48.0);
            *nearby = Some(
                world
                    .get_entities_at_box(&area)
                    .iter()
                    .filter(|e| e.get_living_entity().is_some())
                    .map(Arc::downgrade)
                    .collect(),
            );
        }
        for entity in
            Self::alive_within(nearby.as_deref().unwrap_or_default(), &self.position, 32.0)
        {
            if let Some(mob) = entity.get_mob() {
                mob.get_mob_entity().with_brain(mob, |brain| {
                    brain.set(HEARD_BELL_TIME, game_time);
                });
            }
        }
    }

    /// Alive, not removed entities whose position is closer than `range` to the block center.
    fn alive_within(
        entities: &[std::sync::Weak<dyn EntityBase>],
        position: &BlockPos,
        range: f64,
    ) -> Vec<Arc<dyn EntityBase>> {
        let center = position.to_centered_f64();
        entities
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .filter(|e| {
                let entity = e.get_entity();
                e.get_living_entity()
                    .is_some_and(|l| !l.dead.load(std::sync::atomic::Ordering::Relaxed))
                    && !entity.is_removed()
                    && entity.pos.load().squared_distance_to_vec(&center) < range * range
            })
            .collect()
    }

    /// Vanilla `areRaidersNearby`.
    fn raiders_hear_bell(&self) -> bool {
        let nearby = self
            .nearby_entities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Self::alive_within(nearby.as_deref().unwrap_or_default(), &self.position, 32.0)
            .iter()
            .any(|e| {
                e.get_entity()
                    .entity_type
                    .has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
            })
    }

    /// Vanilla `makeRaidersGlow`, the server's resonation end action.
    fn make_raiders_glow(&self) {
        let nearby = self
            .nearby_entities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or_default();
        for raider in Self::alive_within(&nearby, &self.position, 48.0) {
            if !raider
                .get_entity()
                .entity_type
                .has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
            {
                continue;
            }
            // Paper `BellRevealRaiderEvent`
            let world = raider.get_entity().world.load_full();
            if world
                .plugin_event(|| {
                    crate::plugin::api::events::block::bell_reveal_raider::BellRevealRaiderEvent {
                        block: self.position,
                        entity: raider.clone(),
                        cancelled: false,
                    }
                })
                .is_some_and(|event| event.cancelled)
            {
                continue;
            }
            if let Some(living) = raider.get_living_entity() {
                living.add_effect(Effect {
                    effect_type: &StatusEffect::GLOWING,
                    duration: 60,
                    amplifier: 0,
                    ambient: false,
                    show_particles: true,
                    show_icon: true,
                    blend: false,
                });
            }
        }
    }
}

impl BlockEntity for BellBlockEntity {
    fn write_nbt(&self, _nbt: &mut NbtCompound) {}

    fn from_nbt(_nbt: &NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        Self::new(position)
    }

    fn tick(&self, world: &Arc<World>) {
        if self.ringing.load() {
            self.ring_ticks.fetch_add(1);
        }
        if self.ring_ticks.load() >= 50 {
            self.ringing.store(false);
            self.ring_ticks.store(0);
        }
        if self.ring_ticks.load() >= 5 && self.resonate_time.load() == 0 && self.raiders_hear_bell()
        {
            self.resonating.store(true);
            world.play_sound_fine(
                Sound::BlockBellResonate,
                SoundCategory::Blocks,
                &self.position.to_f64(),
                1.0,
                1.0,
            );
        }

        if self.resonating.load() {
            if self.resonate_time.load() < 40 {
                self.resonate_time.fetch_add(1);
            } else {
                let mut event =
                    crate::plugin::api::events::block::bell_resonate::BellResonateEvent {
                        block_pos: self.position,
                        world: world.clone(),
                        cancelled: false,
                    };
                if !world.fire_plugin_event(&mut event) {
                    self.make_raiders_glow();
                }
                self.resonating.store(false);
            }
        }
    }

    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn bedrock_block_actor_data(
        &self,
        _state_id: pumpkin_data::BlockStateId,
    ) -> Option<NbtCompound> {
        let direction = match self.last_side_hit.load() {
            Some(HorizontalFacing::South) => 0,
            Some(HorizontalFacing::West) => 1,
            Some(HorizontalFacing::North) => 2,
            Some(HorizontalFacing::East) => 3,
            None => 255,
        };
        Some(
            super::bedrock::BedrockBlockActorBuilder::new("Bell", self.position)
                .movable(false)
                .put_bool("Ringing", self.ringing.load())
                .put_int("Ticks", self.ring_ticks.load())
                .put_int("Direction", direction)
                .finish(),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
