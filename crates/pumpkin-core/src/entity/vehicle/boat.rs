use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crossbeam::atomic::AtomicCell;

use crate::entity::player::Player;
use crate::entity::vehicle::minecart::container::{self, MinecartInventory};
use crate::entity::{Entity, EntityBase, living::LivingEntity};
use crate::server::Server;
use crate::world::World;
use crate::world::game_event::GameEventContext;

use pumpkin_data::Block;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::translation;
use pumpkin_inventory::Inventory;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::Metadata;
use pumpkin_util::GameMode;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::text::TextComponent;

use crate::entity::vehicle::vehicle::VehicleEntity;

const FRONT_SEAT_OFFSET: f32 = 0.2;
const BACK_SEAT_OFFSET: f32 = -0.6;
const ANIMAL_SEAT_SHIFT: f32 = 0.2;
const CHEST_BOAT_CONTAINER_SIZE: usize = 27;
/// Vanilla `AbstractBoat.TIME_TO_EJECT`.
const TIME_TO_EJECT: f32 = 60.0;
/// Vanilla `AbstractBoat.getDefaultGravity`.
const DEFAULT_GRAVITY: f64 = 0.04;

/// Vanilla `AbstractBoat.Status`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BoatStatus {
    InWater,
    UnderWater,
    UnderFlowingWater,
    OnLand,
    InAir,
}

#[must_use]
pub const fn is_chest_boat(entity_type: &EntityType) -> bool {
    entity_type.id == EntityType::ACACIA_CHEST_BOAT.id
        || entity_type.id == EntityType::BAMBOO_CHEST_RAFT.id
        || entity_type.id == EntityType::BIRCH_CHEST_BOAT.id
        || entity_type.id == EntityType::CHERRY_CHEST_BOAT.id
        || entity_type.id == EntityType::DARK_OAK_CHEST_BOAT.id
        || entity_type.id == EntityType::JUNGLE_CHEST_BOAT.id
        || entity_type.id == EntityType::MANGROVE_CHEST_BOAT.id
        || entity_type.id == EntityType::OAK_CHEST_BOAT.id
        || entity_type.id == EntityType::PALE_OAK_CHEST_BOAT.id
        || entity_type.id == EntityType::POPLAR_CHEST_BOAT.id
        || entity_type.id == EntityType::SPRUCE_CHEST_BOAT.id
}

pub struct BoatEntity {
    pub vehicle: VehicleEntity,
    /// Vanilla `outOfControlTicks`: counts while under water, passengers are ejected at 60.
    out_of_control_ticks: AtomicCell<f32>,
    status: AtomicCell<Option<BoatStatus>>,
    old_status: AtomicCell<Option<BoatStatus>>,
    water_level: AtomicCell<f64>,
    land_friction: AtomicCell<f32>,
    last_yd: AtomicCell<f64>,
    left_paddle_moving: AtomicBool,
    right_paddle_moving: AtomicBool,
    /// Vanilla `isAboveBubbleColumn` / `bubbleColumnDirectionIsDown`, set by the column this tick.
    above_bubble_column: AtomicBool,
    bubble_column_down: AtomicBool,
    /// Vanilla synced `DATA_ID_BUBBLE_TIME`.
    bubble_time: AtomicI32,
    inventory: Option<Arc<MinecartInventory>>,
}

impl BoatEntity {
    #[must_use]
    pub fn new(mut entity: Entity) -> Self {
        entity.blocks_building = true;
        let inventory = is_chest_boat(entity.entity_type)
            .then(|| Arc::new(MinecartInventory::new(CHEST_BOAT_CONTAINER_SIZE)));
        Self {
            vehicle: VehicleEntity::new(entity),
            out_of_control_ticks: AtomicCell::new(0.0),
            status: AtomicCell::new(None),
            old_status: AtomicCell::new(None),
            water_level: AtomicCell::new(0.0),
            land_friction: AtomicCell::new(0.0),
            last_yd: AtomicCell::new(0.0),
            left_paddle_moving: AtomicBool::new(false),
            right_paddle_moving: AtomicBool::new(false),
            above_bubble_column: AtomicBool::new(false),
            bubble_column_down: AtomicBool::new(false),
            bubble_time: AtomicI32::new(0),
            inventory,
        }
    }

    const fn max_passengers(&self) -> usize {
        if self.inventory.is_some() { 1 } else { 2 }
    }

    pub fn open_custom_inventory_screen(&self, player: &Arc<Player>) -> bool {
        let Some(inventory) = &self.inventory else {
            return false;
        };
        let entity_type = self.vehicle.entity.entity_type;
        let bedrock_key = if entity_type.id == EntityType::BAMBOO_CHEST_RAFT.id {
            translation::bedrock::ENTITY_CHEST_RAFT_NAME
        } else {
            translation::bedrock::ENTITY_CHEST_BOAT_NAME
        };
        let title = TextComponent::translate_cross(
            format!("entity.minecraft.{}", entity_type.resource_name),
            bedrock_key,
            [],
        );
        let opened = container::open(&self.vehicle.entity, player, inventory, title, false);
        if opened {
            let world = self.vehicle.entity.world.load();
            world.emit_game_event_with_context(
                GameEvent::ContainerOpen.name(),
                self.vehicle.entity.pos.load(),
                &GameEventContext {
                    source_entity: Some(player.as_ref()),
                },
            );
        }
        opened
    }

    pub fn set_paddles(&self, left: bool, right: bool) {
        self.left_paddle_moving.store(left, Ordering::Relaxed);
        self.right_paddle_moving.store(right, Ordering::Relaxed);

        self.vehicle.entity.send_meta_data(
            &[
                Metadata::new(pumpkin_data::tracked_data::boat::ID_PADDLE_LEFT, left),
                Metadata::new(pumpkin_data::tracked_data::boat::ID_PADDLE_RIGHT, right),
            ],
            None,
        );
    }

    fn send_wobble_metadata(&self) {
        self.vehicle.send_wobble_metadata();
    }

    /// Height of the water surface in the block at `pos` and whether it is a source,
    /// or `None` when the block holds no water (`FluidState.is(FluidTags.WATER)`).
    fn water_at(world: &World, pos: &BlockPos) -> Option<(f32, bool)> {
        let (fluid, state) = world.get_fluid_and_fluid_state(pos);
        if state.is_empty || !world.fluid_state_has_tag(pos, &tag::Fluid::MINECRAFT_WATER) {
            return None;
        }
        Some((world.get_fluid_height(pos, fluid, &state), state.is_source))
    }

    /// Vanilla `AbstractBoat.getStatus`.
    fn compute_status(&self, world: &World) -> BoatStatus {
        if let Some(status) = self.is_underwater(world) {
            self.water_level
                .store(self.vehicle.entity.bounding_box.load().max.y);
            return status;
        }
        if self.check_in_water(world) {
            return BoatStatus::InWater;
        }
        let friction = self.ground_friction(world);
        // NaN (no blocks below) fails this check, like vanilla's `fcmpl`.
        if friction > 0.0 {
            self.land_friction.store(friction);
            return BoatStatus::OnLand;
        }
        BoatStatus::InAir
    }

    /// Vanilla `AbstractBoat.isUnderwater`.
    fn is_underwater(&self, world: &World) -> Option<BoatStatus> {
        let bb = self.vehicle.entity.bounding_box.load();
        let top = bb.max.y + 0.001;
        let min_x = bb.min.x.floor() as i32;
        let max_x = bb.max.x.ceil() as i32;
        let min_y = bb.max.y.floor() as i32;
        let max_y = top.ceil() as i32;
        let min_z = bb.min.z.floor() as i32;
        let max_z = bb.max.z.ceil() as i32;
        let mut under_water = false;
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    let pos = BlockPos::new(x, y, z);
                    if let Some((height, is_source)) = Self::water_at(world, &pos)
                        && top < f64::from(y as f32 + height)
                    {
                        if !is_source {
                            return Some(BoatStatus::UnderFlowingWater);
                        }
                        under_water = true;
                    }
                }
            }
        }
        under_water.then_some(BoatStatus::UnderWater)
    }

    /// Vanilla `AbstractBoat.checkInWater`: also records the highest water surface as `waterLevel`.
    fn check_in_water(&self, world: &World) -> bool {
        let bb = self.vehicle.entity.bounding_box.load();
        let min_x = bb.min.x.floor() as i32;
        let max_x = bb.max.x.ceil() as i32;
        let min_y = bb.min.y.floor() as i32;
        let max_y = (bb.min.y + 0.001).ceil() as i32;
        let min_z = bb.min.z.floor() as i32;
        let max_z = bb.max.z.ceil() as i32;
        let mut in_water = false;
        let mut water_level = -f64::MAX;
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    let pos = BlockPos::new(x, y, z);
                    let Some((height, _)) = Self::water_at(world, &pos) else {
                        continue;
                    };
                    let surface = y as f32 + height;
                    water_level = f64::from(surface).max(water_level);
                    in_water |= bb.min.y < f64::from(surface);
                }
            }
        }
        self.water_level.store(water_level);
        in_water
    }

    /// Vanilla `AbstractBoat.getGroundFriction`: the average friction of the blocks whose
    /// collision shape touches a 0.001 thin box under the boat, `NaN` when there are none.
    fn ground_friction(&self, world: &World) -> f32 {
        let bb = self.vehicle.entity.bounding_box.load();
        let thin = BoundingBox::new(
            Vector3::new(bb.min.x, bb.min.y - 0.001, bb.min.z),
            Vector3::new(bb.max.x, bb.min.y, bb.max.z),
        );
        let min_x = thin.min.x.floor() as i32 - 1;
        let max_x = thin.max.x.ceil() as i32 + 1;
        let min_y = thin.min.y.floor() as i32 - 1;
        let max_y = thin.max.y.ceil() as i32 + 1;
        let min_z = thin.min.z.floor() as i32 - 1;
        let max_z = thin.max.z.ceil() as i32 + 1;
        let mut friction = 0.0f32;
        let mut count = 0i32;
        for x in min_x..max_x {
            for z in min_z..max_z {
                let edges = i32::from(x == min_x || x == max_x - 1)
                    + i32::from(z == min_z || z == max_z - 1);
                if edges == 2 {
                    continue;
                }
                for y in min_y..max_y {
                    if edges > 0 && (y == min_y || y == max_y - 1) {
                        continue;
                    }
                    let pos = BlockPos::new(x, y, z);
                    let (block, state) = world.get_block_and_state(&pos);
                    if block.id == Block::LILY_PAD.id {
                        continue;
                    }
                    if state
                        .get_block_collision_shapes_at(&pos)
                        .any(|shape| shape.at_pos(pos).intersects(&thin))
                    {
                        friction += block.slipperiness;
                        count += 1;
                    }
                }
            }
        }
        friction / count as f32
    }

    /// Vanilla `AbstractBoat.getWaterLevelAbove`.
    fn water_level_above(&self, world: &World) -> f32 {
        let bb = self.vehicle.entity.bounding_box.load();
        let min_x = bb.min.x.floor() as i32;
        let max_x = bb.max.x.ceil() as i32;
        let min_y = bb.max.y.floor() as i32;
        let max_y = (bb.max.y - self.last_yd.load()).ceil() as i32;
        let min_z = bb.min.z.floor() as i32;
        let max_z = bb.max.z.ceil() as i32;
        'layers: for y in min_y..max_y {
            let mut height = 0.0f32;
            for x in min_x..max_x {
                for z in min_z..max_z {
                    if let Some((h, _)) = Self::water_at(world, &BlockPos::new(x, y, z)) {
                        height = height.max(h);
                    }
                    if height >= 1.0 {
                        continue 'layers;
                    }
                }
            }
            if height < 1.0 {
                return y as f32 + height;
            }
        }
        (max_y + 1) as f32
    }

    /// Vanilla `AbstractBoat.floatBoat`.
    fn float_boat(&self, world: &World, status: BoatStatus) {
        let entity = &self.vehicle.entity;
        let mut gravity = -self.get_gravity();
        let mut buoyancy = 0.0f64;
        let inertia: f32;
        if self.old_status.load() == Some(BoatStatus::InAir)
            && status != BoatStatus::InAir
            && status != BoatStatus::OnLand
        {
            self.water_level.store(entity.get_y(1.0));
            let target_y = f64::from(self.water_level_above(world) - entity.height()) + 0.101;
            let pos = entity.pos.load();
            let moved = entity
                .bounding_box
                .load()
                .shift(Vector3::new(0.0, target_y - pos.y, 0.0));
            if world.no_collision(self, moved) {
                entity.set_pos(Vector3::new(pos.x, target_y, pos.z));
                entity
                    .velocity
                    .store(entity.velocity.load().multiply(1.0, 0.0, 1.0));
                self.last_yd.store(0.0);
            }
            self.status.store(Some(BoatStatus::InWater));
            return;
        }
        match status {
            BoatStatus::InWater => {
                buoyancy =
                    (self.water_level.load() - entity.pos.load().y) / f64::from(entity.height());
                inertia = 0.9;
            }
            BoatStatus::UnderFlowingWater => {
                gravity = -7.0e-4;
                inertia = 0.9;
            }
            BoatStatus::UnderWater => {
                buoyancy = f64::from(0.01f32);
                inertia = 0.45;
            }
            BoatStatus::InAir => inertia = 0.9,
            BoatStatus::OnLand => {
                inertia = self.land_friction.load();
                if self
                    .get_controlling_passenger()
                    .is_some_and(|passenger| passenger.get_player().is_some())
                {
                    self.land_friction.store(inertia / 2.0);
                }
            }
        }
        let velocity = entity.velocity.load();
        let inertia = f64::from(inertia);
        let mut velocity = Vector3::new(
            velocity.x * inertia,
            velocity.y + gravity,
            velocity.z * inertia,
        );
        if buoyancy > 0.0 {
            velocity.y = (velocity.y + buoyancy * (DEFAULT_GRAVITY / 0.65)) * 0.75;
        }
        entity.velocity.store(velocity);
    }

    fn set_bubble_time(&self, time: i32) {
        if self.bubble_time.swap(time, Ordering::Relaxed) != time {
            self.vehicle.entity.send_meta_data(
                &[Metadata::new(
                    pumpkin_data::tracked_data::boat::ID_BUBBLE_TIME,
                    VarInt(time),
                )],
                None,
            );
        }
    }

    /// Vanilla `AbstractBoat.tickBubbleColumn` for the server.
    fn tick_bubble_column(&self, controlled_by_player: bool) {
        if !self.above_bubble_column.load(Ordering::Relaxed) {
            self.set_bubble_time(0);
        }
        let mut time = self.bubble_time.load(Ordering::Relaxed);
        if time > 0 {
            time -= 1;
            self.set_bubble_time(time);
            if 60 - time - 1 > 0 && time == 0 {
                self.set_bubble_time(0);
                let entity = &self.vehicle.entity;
                let world = entity.world.load();
                let down = self.bubble_column_down.load(Ordering::Relaxed);
                if down {
                    self.eject_passengers();
                    world.send_entity_status(entity, EntityStatus::BoatSink, ActorEventID::None);
                } else {
                    world.send_entity_status(entity, EntityStatus::BoatLaunch, ActorEventID::None);
                }
                // `handleBubbleColumnEffect`, skipped when the client simulates the boat.
                if !controlled_by_player {
                    let velocity = entity.velocity.load();
                    entity.velocity.store(if down {
                        Vector3::new(velocity.x, velocity.y - 0.7, velocity.z)
                    } else {
                        Vector3::new(velocity.x, 0.6, velocity.z)
                    });
                }
            }
            self.above_bubble_column.store(false, Ordering::Relaxed);
        }
    }

    /// The pickup and push loop at the end of vanilla `AbstractBoat.tick`.
    fn push_and_pickup_entities(&self) {
        let entity = &self.vehicle.entity;
        let world = entity.world.load();
        let bounding_box = entity.bounding_box.load();
        let area = bounding_box.expand(0.2, -0.01, 0.2);
        let team = self.get_team();
        let others: Vec<Arc<dyn EntityBase>> = world
            .get_all_at_box(&area)
            .into_iter()
            .filter(|other| {
                other.get_entity().entity_id != entity.entity_id
                    && crate::entity::is_pushable_by(team.as_ref(), other.as_ref())
            })
            .collect();
        if others.is_empty() {
            return;
        }
        let can_pickup = self
            .get_controlling_passenger()
            .is_none_or(|passenger| passenger.get_player().is_none());
        let width = f64::from(entity.width());
        for other in others {
            if self.has_passenger(other.as_ref()) {
                continue;
            }
            let other_entity = other.get_entity();
            let passenger_count = entity
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len();
            if can_pickup
                && passenger_count < self.max_passengers()
                && !other.is_passenger()
                // `hasEnoughSpaceFor`
                && f64::from(other_entity.width()) < width
                && other.get_living_entity().is_some()
                && !other_entity
                    .entity_type
                    .has_tag(&tag::EntityType::MINECRAFT_CANNOT_BE_PUSHED_ONTO_BOATS)
            {
                if let Some(me) = world.get_entity_by_id(entity.entity_id) {
                    // `canAddPassenger`: a free seat and the boat's eyes out of water.
                    let can_add =
                        passenger_count < self.max_passengers() && !entity.is_submerged_in_water();
                    crate::entity::start_riding(&other, &me, can_add);
                }
            } else {
                self.push(other.as_ref());
            }
        }
    }

    /// Vanilla `Entity.ejectPassengers`.
    fn eject_passengers(&self) {
        let ids: Vec<i32> = self
            .vehicle
            .entity
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|passenger| passenger.get_entity().entity_id)
            .collect();
        for id in ids {
            self.vehicle.entity.remove_passenger(id);
        }
    }
}

impl EntityBase for BoatEntity {
    fn passenger_attachment_point(
        &self,
        passenger: &dyn EntityBase,
        index: usize,
    ) -> Option<Vector3<f64>> {
        let entity = self.get_entity();
        let seats = entity.entity_type.boat?;
        let mut offset = seats.single_passenger_x_offset;
        let passenger_count = entity
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        if passenger_count > 1 {
            offset = if index == 0 {
                FRONT_SEAT_OFFSET
            } else {
                BACK_SEAT_OFFSET
            };
            if passenger
                .get_mob()
                .and_then(|mob| mob.as_animal())
                .is_some()
            {
                offset += ANIMAL_SEAT_SHIFT;
            }
        }
        Some(crate::entity::attachment_point(
            [0.0, seats.ride_height, f64::from(offset)],
            1.0,
            entity.yaw.load(),
        ))
    }

    fn get_controlling_passenger(&self) -> Option<Arc<dyn EntityBase>> {
        self.get_entity()
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first()
            .filter(|passenger| passenger.get_living_entity().is_some())
            .cloned()
    }

    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(inventory) = &self.inventory {
            inventory.write_nbt(nbt);
        }
    }

    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        if let Some(inventory) = &self.inventory {
            inventory.read_nbt(nbt);
        }
    }

    fn get_entity(&self) -> &Entity {
        &self.vehicle.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn has_container_inventory(&self) -> bool {
        self.inventory.is_some()
    }

    fn get_container_inventory(&self) -> Option<Arc<dyn Inventory>> {
        let container = self.inventory.as_ref()?;
        if container.has_loot_table() {
            let world = self.vehicle.entity.world.load();
            container.unpack_loot(&crate::world::loot::LootContextParameters {
                world: Some(&world),
                position: Some(self.vehicle.entity.pos.load()),
                ..Default::default()
            });
        }
        Some(container.clone() as Arc<dyn Inventory>)
    }

    /// Vanilla `AbstractBoat.tick` for the server side.
    fn tick(&self, caller: &dyn EntityBase, server: &Server) {
        let entity = &self.vehicle.entity;
        let world = entity.world.load();

        self.old_status.store(self.status.load());
        let status = self.compute_status(&world);
        self.status.store(Some(status));
        if matches!(
            status,
            BoatStatus::UnderWater | BoatStatus::UnderFlowingWater
        ) {
            self.out_of_control_ticks
                .store(self.out_of_control_ticks.load() + 1.0);
        } else {
            self.out_of_control_ticks.store(0.0);
        }
        if self.out_of_control_ticks.load() >= TIME_TO_EJECT {
            self.eject_passengers();
        }

        // Hurt time / damage decay, then vanilla `super.tick()` (`Entity.baseTick`).
        self.vehicle.tick();
        entity.tick(caller, server);
        if entity.is_removed() {
            return;
        }

        // `isLocalInstanceAuthoritative`: a player controlling the boat moves it client-side.
        let controlled_by_player = self
            .get_controlling_passenger()
            .is_some_and(|passenger| passenger.get_player().is_some());
        if controlled_by_player {
            entity.velocity.store(Vector3::new(0.0, 0.0, 0.0));
        } else {
            let first_is_player = entity
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .first()
                .is_some_and(|passenger| passenger.get_player().is_some());
            // `setPaddleState(false, false)` only syncs a change.
            if !first_is_player
                && (self.left_paddle_moving.load(Ordering::Relaxed)
                    || self.right_paddle_moving.load(Ordering::Relaxed))
            {
                self.set_paddles(false, false);
            }
            self.float_boat(&world, status);
            let velocity = entity.velocity.load();
            // `checkFallDamage` records the motion handed to `move` as `lastYd`.
            self.last_yd.store(velocity.y);
            entity.move_entity(caller, velocity);
        }

        // Vanilla applies block effects twice, then the bubble column.
        entity.tick_block_collisions(caller);
        if entity.is_removed() {
            return;
        }
        entity.tick_block_collisions(caller);
        if entity.is_removed() {
            return;
        }
        self.tick_bubble_column(controlled_by_player);
        self.push_and_pickup_entities();
    }

    /// Vanilla `AbstractBoat.push`: another boat only when it is below this one's top,
    /// anything else only when its feet are not above this boat's bottom.
    fn push(&self, other: &dyn EntityBase) {
        let own_box = self.vehicle.entity.bounding_box.load();
        let other_entity = other.get_entity();
        let other_box = other_entity.bounding_box.load();
        let pushes = if other_entity.entity_type.boat.is_some() {
            other_box.min.y < own_box.max.y
        } else {
            other_box.min.y <= own_box.min.y
        };
        if pushes {
            crate::entity::entity_push(self, other);
        }
    }

    /// Vanilla `AbstractBoat.onAboveBubbleColumn`: the column only arms the bubble timer.
    fn on_above_bubble_column(&self, drag_down: bool, _pos: &BlockPos) {
        self.above_bubble_column.store(true, Ordering::Relaxed);
        self.bubble_column_down.store(drag_down, Ordering::Relaxed);
        if self.bubble_time.load(Ordering::Relaxed) == 0 {
            self.set_bubble_time(60);
        }
    }

    fn get_gravity(&self) -> f64 {
        if self.vehicle.entity.has_no_gravity() {
            0.0
        } else {
            DEFAULT_GRAVITY
        }
    }

    fn init_data_tracker(&self) {
        self.send_wobble_metadata();
    }

    fn can_hit(&self) -> bool {
        self.vehicle.entity.is_alive()
    }

    fn is_collidable(&self, _entity: Option<Box<dyn EntityBase>>) -> bool {
        true
    }

    fn damage_with_context(
        &self,
        _caller: &dyn EntityBase,
        amount: f32,
        _damage_type: DamageType,
        _position: Option<Vector3<f64>>,
        source: Option<&dyn EntityBase>,
        _cause: Option<&dyn EntityBase>,
    ) -> bool {
        let creative = source
            .and_then(EntityBase::get_player)
            .is_some_and(|player| player.gamemode.load() == GameMode::Creative);
        let will_break = self.vehicle.entity.is_alive()
            && (creative || self.vehicle.get_damage() + amount * 10.0 > 40.0);

        let damaged = self.vehicle.damage_with_context(amount, source);

        if will_break && !creative && self.vehicle.entity.is_removed() {
            let world = self.vehicle.entity.world.load();
            // The boat item itself was already dropped by `VehicleEntity::kill_and_drop_self`.
            if world.level_info.load().game_rules.entity_drops
                && let Some(container) = &self.inventory
                && container.claim_drops()
            {
                let position = self.vehicle.entity.block_pos.load();
                container.unpack_loot(&crate::world::loot::LootContextParameters {
                    world: Some(&world),
                    position: Some(self.vehicle.entity.pos.load()),
                    ..Default::default()
                });
                let inventory: Arc<dyn Inventory> = container.clone();
                world.scatter_inventory(&position, &inventory);
            }
            // Vanilla `chestVehicleDestroyed`: a player breaking it angers piglins
            if self.inventory.is_some()
                && let Some(player) = source
                    .and_then(EntityBase::get_player)
                    .and_then(|player| world.get_player_by_uuid(player.gameprofile.id))
            {
                crate::entity::mob::piglin_ai::anger_nearby_piglins(&world, &player, true);
            }
        }

        damaged
    }

    fn interact(&self, player: &Arc<Player>, _item_stack: &mut ItemStack) -> bool {
        let can_add_passenger = self.out_of_control_ticks.load() < TIME_TO_EJECT
            && !player.get_entity().has_vehicle()
            && self
                .vehicle
                .entity
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len()
                < self.max_passengers();

        if self.inventory.is_some() && (!can_add_passenger || player.get_entity().is_sneaking()) {
            let _ = self.open_custom_inventory_screen(player);
            return true;
        }

        if player.get_entity().is_sneaking() || !can_add_passenger {
            return false;
        }

        let world = self.vehicle.entity.world.load();
        let Some(vehicle) = world.get_entity_by_id(self.vehicle.entity.entity_id) else {
            return false;
        };

        let Some(passenger) = world.get_player_by_id(player.entity_id()) else {
            return false;
        };

        self.vehicle
            .entity
            .add_passenger(vehicle, passenger as Arc<dyn EntityBase>);

        true
    }

    fn set_paddle_state(&self, left: bool, right: bool) {
        self.set_paddles(left, right);
    }
    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }

    fn is_pushable(&self) -> bool {
        true
    }

    fn can_be_collided_with(&self, _other: &dyn EntityBase) -> bool {
        true
    }

    fn collides_with_pushables(&self) -> bool {
        true
    }
}
