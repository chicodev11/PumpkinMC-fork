use super::projectile::owner::EntityReference;
use super::{Entity, EntityBase, living::LivingEntity};
use crate::server::Server;
use crate::world::World;
use crate::world::explosion::ExplosionDamageCalculator;
use core::f32;
use crossbeam::atomic::AtomicCell;
use pumpkin_data::Block;
use pumpkin_data::entity::EntityType;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use std::{
    f64::consts::TAU,
    sync::{
        Arc,
        atomic::{
            AtomicU32,
            Ordering::{self, Relaxed},
        },
    },
};

/// Vanilla `Entity.getAirDrag`, applied every tick.
const AIR_DRAG: f32 = 0.98;

/// Vanilla `PrimedTnt.USED_PORTAL_DAMAGE_CALCULATOR`: TNT that came through a portal does not
/// break the nether portal it lands next to.
struct UsedPortalDamageCalculator;

impl ExplosionDamageCalculator for UsedPortalDamageCalculator {
    fn get_block_explosion_resistance(
        &self,
        explosion: &crate::world::explosion::Explosion<'_>,
        world: &World,
        pos: &BlockPos,
        block: &Block,
        fluid: &pumpkin_data::fluid::FluidState,
    ) -> Option<f32> {
        if block == &Block::NETHER_PORTAL {
            return None;
        }
        crate::world::explosion::DefaultExplosionDamageCalculator
            .get_block_explosion_resistance(explosion, world, pos, block, fluid)
    }

    fn should_block_explode(
        &self,
        explosion: &crate::world::explosion::Explosion<'_>,
        world: &World,
        pos: &BlockPos,
        block: &Block,
        power: f32,
    ) -> bool {
        block != &Block::NETHER_PORTAL
            && crate::world::explosion::DefaultExplosionDamageCalculator
                .should_block_explode(explosion, world, pos, block, power)
    }
}

pub struct TNTEntity {
    entity: Entity,
    power: AtomicCell<f32>,
    fuse: AtomicU32,
    /// Vanilla `DATA_BLOCK_STATE_ID`: the block the primed TNT renders as.
    block_state: AtomicCell<pumpkin_data::BlockStateId>,
    /// Vanilla `usedPortal`, not saved.
    used_portal: std::sync::atomic::AtomicBool,
    /// Vanilla `PrimedTnt.owner`: who lit it, credited for the explosion.
    owner: EntityReference,
}

impl TNTEntity {
    /// Vanilla `PrimedTnt.DEFAULT_FUSE_TIME`.
    pub const DEFAULT_FUSE: u32 = 80;
    /// Vanilla `PrimedTnt.DEFAULT_EXPLOSION_POWER`.
    pub const DEFAULT_POWER: f32 = 4.0;

    pub const fn new(mut entity: Entity, power: f32, fuse: u32) -> Self {
        entity.blocks_building = true;
        Self {
            entity,
            power: AtomicCell::new(power),
            fuse: AtomicU32::new(fuse),
            block_state: AtomicCell::new(Block::TNT.default_state.id),
            used_portal: std::sync::atomic::AtomicBool::new(false),
            owner: EntityReference::none(),
        }
    }

    /// Records who lit the TNT; vanilla only keeps living owners.
    pub fn set_owner(&self, owner: Option<&dyn EntityBase>) {
        self.owner.set(
            owner
                .filter(|owner| owner.get_living_entity().is_some())
                .map(EntityBase::get_entity),
        );
    }

    pub(crate) fn is_about_to_explode(&self) -> bool {
        self.fuse.load(Relaxed) <= 1
    }

    /// Vanilla primed-TNT constructor: block centre plus the random launch impulse.
    pub fn primed(
        world: &Arc<World>,
        position: &BlockPos,
        fuse: u32,
        owner: Option<&Entity>,
    ) -> Arc<Self> {
        let pos = Vector3::new(
            f64::from(position.0.x) + 0.5,
            f64::from(position.0.y),
            f64::from(position.0.z) + 0.5,
        );
        let entity = Entity::new(world.clone(), pos, &EntityType::TNT);
        // Note (RNG source, FORK_TODO): vanilla rolls from `level.random`; same odds, not seed-reproducible.
        let rot = rand::random::<f64>() * TAU;
        entity
            .velocity
            .store(Vector3::new(-rot.sin() * 0.02, 0.2, -rot.cos() * 0.02));
        let tnt = Self::new(entity, Self::DEFAULT_POWER, fuse);
        tnt.owner.set_uuid(owner.map(|owner| owner.entity_uuid));
        Arc::new(tnt)
    }

    /// Vanilla `PrimedTnt.getRandomShortFuse`.
    #[must_use]
    pub fn random_short_fuse(fuse: u32) -> u32 {
        rand::random_range(0..(fuse / 4).max(1)) + fuse / 8
    }
}

impl EntityBase for TNTEntity {
    fn can_hit(&self) -> bool {
        !self.get_entity().is_removed()
    }

    /// Vanilla `PrimedTnt.addAdditionalSaveData`. Without it a chunk reload resets a
    /// nearly-detonated fuse and the TNT explodes late.
    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_short("fuse", self.fuse.load(Relaxed) as i16);
        nbt.put_compound(
            "block_state",
            pumpkin_world::block::BlockStateCodec::state_to_nbt(self.block_state.load()),
        );
        let power = self.power.load();
        if (power - Self::DEFAULT_POWER).abs() > f32::EPSILON {
            nbt.put_float("explosion_power", power);
        }
        self.owner.write_nbt(nbt, "owner");
    }

    /// Vanilla `PrimedTnt.readAdditionalSaveData`.
    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        let fuse = nbt
            .get_numeric_short("fuse")
            .map_or(Self::DEFAULT_FUSE, |fuse| fuse.max(0) as u32);
        self.fuse.store(fuse, Relaxed);
        let block_state = nbt
            .get_compound("block_state")
            .and_then(pumpkin_world::block::BlockStateCodec::state_from_nbt)
            .unwrap_or(Block::TNT.default_state.id);
        self.block_state.store(block_state);
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::tnt::BLOCK_STATE_ID,
            VarInt(i32::from(block_state.as_u16())),
        );
        self.power.store(
            nbt.get_numeric_float("explosion_power")
                .unwrap_or(Self::DEFAULT_POWER)
                .clamp(0.0, 128.0),
        );
        self.owner.read_nbt(nbt, "owner");
    }

    fn get_tnt_owner(&self) -> Option<Arc<dyn EntityBase>> {
        self.owner
            .get(&self.entity.world.load())
            .filter(|owner| owner.get_living_entity().is_some())
    }

    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        let entity = &self.entity;

        let mut velo = entity.velocity.load();
        velo.y -= self.get_gravity();
        entity.velocity.store(velo);

        entity.move_entity(caller, velo);
        entity.tick_block_collisions(caller);
        // Vanilla `setRequiresPrecisePosition(horizontalCollision || verticalCollision)`.
        entity.requires_precise_position.store(
            entity.horizontal_collision.load(Relaxed) || entity.vertical_collision.load(Relaxed),
            Relaxed,
        );

        // Read back what actually happened instead of reusing the pre-move value:
        // `move_entity` clamps on collision, and an explosion may have pushed us while we
        // were moving above. Air drag applies every tick, the ground factors on top of it.
        let drag = f64::from(AIR_DRAG);
        let velo = entity.velocity.load().multiply(drag, drag, drag);
        let velo = if entity.on_ground.load(Ordering::Relaxed) {
            velo.multiply(0.7, -0.5, 0.7)
        } else {
            velo
        };
        entity.velocity.store(velo);

        // Vanilla `setFuse` every tick; the tracker picks it up through `is_dirty`.
        let fuse = self.fuse.load(Relaxed).saturating_sub(1);
        self.fuse.store(fuse, Relaxed);
        entity.set_synced_data(
            pumpkin_data::tracked_data::tnt::FUSE_ID,
            VarInt(fuse as i32),
        );

        if fuse == 0 {
            entity.remove();
            let world = entity.world.load_full();
            if world.level_info.load().game_rules.tnt_explodes {
                // Vanilla `PrimedTnt.explode`: `getY(0.0625)`.
                let pos = entity.pos.load();
                let y = f64::from(entity.entity_type.dimension[1]).mul_add(0.0625, pos.y);
                let mut options = crate::world::explosion::ExplosionOptions::from_source(caller);
                if self.used_portal.load(Relaxed) {
                    options.damage_calculator = Some(Arc::new(UsedPortalDamageCalculator));
                }
                world.explode_with(
                    Vector3::new(pos.x, y, pos.z),
                    self.power.load(),
                    crate::world::ExplosionInteraction::Tnt,
                    options,
                );
            }
        } else {
            entity.update_fluid_state(caller);
        }
    }

    fn init_data_tracker(&self) {
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::tnt::FUSE_ID,
            VarInt(self.fuse.load(Relaxed) as i32),
        );
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::tnt::BLOCK_STATE_ID,
            VarInt(i32::from(self.block_state.load().as_u16())),
        );
    }

    fn on_portal_teleport(&self) {
        self.used_portal.store(true, Relaxed);
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn get_gravity(&self) -> f64 {
        0.04
    }

    fn entity_bedrock_spawn_metadata(
        &self,
    ) -> Option<pumpkin_protocol::bedrock::client::set_actor_data::SyncedActorDataList> {
        let mut meta =
            pumpkin_protocol::bedrock::client::set_actor_data::SyncedActorDataList::new();
        let fuse = self.fuse.load(Ordering::Relaxed) as i32;
        meta.set(
            pumpkin_protocol::bedrock::client::set_actor_data::entity_data_key::FUSE_TIME,
            pumpkin_protocol::bedrock::client::set_actor_data::MetadataValue::Int(fuse),
        );
        Some(meta)
    }

    fn bedrock_y_offset(&self) -> f64 {
        0.49
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
}
