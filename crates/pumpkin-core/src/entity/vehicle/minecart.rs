mod chest;
pub mod command_block;
pub(crate) mod container;
mod furnace;
mod hopper;
mod rideable;
mod spawner;
mod tnt;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pumpkin_protocol::java::server::play::SPlayerInput;
use rand::RngExt;

use crate::entity::projectile::abstract_arrow::is_abstract_arrow;
use crate::{
    entity::{Entity, EntityBase, living::LivingEntity, player::Player},
    server::Server,
};
use pumpkin_data::block_properties::{
    PoweredRailLikeProperties, RailLikeProperties, RailShape, RailShapeStraight,
};
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId};
use pumpkin_inventory::Inventory;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::vehicle::vehicle::VehicleEntity;
use chest::ChestMinecart;
use command_block::CommandBlockMinecart;
use container::MinecartInventory;
use furnace::FurnaceMinecart;
use hopper::HopperMinecart;
use rideable::RideableMinecart;
use spawner::SpawnerMinecart;
use tnt::TntMinecart;

/// Vanilla `AbstractMinecart.exits`: the two block offsets a rail shape connects.
const fn get_exits(shape: RailShape) -> ([i32; 3], [i32; 3]) {
    const NORTH: [i32; 3] = [0, 0, -1];
    const SOUTH: [i32; 3] = [0, 0, 1];
    const WEST: [i32; 3] = [-1, 0, 0];
    const EAST: [i32; 3] = [1, 0, 0];
    match shape {
        RailShape::NorthSouth => (NORTH, SOUTH),
        RailShape::EastWest => (WEST, EAST),
        RailShape::AscendingEast => ([-1, -1, 0], EAST),
        RailShape::AscendingWest => (WEST, [1, -1, 0]),
        RailShape::AscendingNorth => (NORTH, [0, -1, 1]),
        RailShape::AscendingSouth => ([0, -1, -1], SOUTH),
        RailShape::SouthEast => (SOUTH, EAST),
        RailShape::SouthWest => (SOUTH, WEST),
        RailShape::NorthWest => (NORTH, WEST),
        RailShape::NorthEast => (NORTH, EAST),
    }
}

/// The shape of a rail block state (`BaseRailBlock.getShapeProperty`).
fn rail_shape(block: &Block, state_id: BlockStateId) -> RailShape {
    if block == &Block::RAIL {
        RailLikeProperties::from_state_id(state_id).shape
    } else {
        match PoweredRailLikeProperties::from_state_id(state_id).shape {
            RailShapeStraight::NorthSouth => RailShape::NorthSouth,
            RailShapeStraight::EastWest => RailShape::EastWest,
            RailShapeStraight::AscendingEast => RailShape::AscendingEast,
            RailShapeStraight::AscendingWest => RailShape::AscendingWest,
            RailShapeStraight::AscendingNorth => RailShape::AscendingNorth,
            RailShapeStraight::AscendingSouth => RailShape::AscendingSouth,
        }
    }
}

fn is_rail(block: &Block) -> bool {
    block.has_tag(&tag::Block::MINECRAFT_RAILS)
}

/// Vanilla `Entity.getInputVector(input, 1.0, yRot)` for a rider's `getLastClientMoveIntent`.
fn rider_move_intent(input: i8, yaw: f32) -> Vector3<f64> {
    let pressed = |flag: i8| input & flag != 0;
    let axis = |positive: bool, negative: bool| -> f64 {
        if positive == negative {
            0.0
        } else if positive {
            1.0
        } else {
            -1.0
        }
    };
    let mut x = axis(pressed(SPlayerInput::LEFT), pressed(SPlayerInput::RIGHT));
    let mut z = axis(
        pressed(SPlayerInput::FORWARD),
        pressed(SPlayerInput::BACKWARD),
    );
    let length_squared = x * x + z * z;
    if length_squared < 1.0e-7 {
        return Vector3::new(0.0, 0.0, 0.0);
    }
    if length_squared > 1.0 {
        let length = length_squared.sqrt();
        x /= length;
        z /= length;
    }
    let radians = yaw * (std::f32::consts::PI / 180.0);
    let sin = f64::from(radians.sin());
    let cos = f64::from(radians.cos());
    Vector3::new(x * cos - z * sin, 0.0, z * cos + x * sin)
}

const fn is_minecart(entity_type: &EntityType) -> bool {
    entity_type.id == EntityType::MINECART.id
        || entity_type.id == EntityType::CHEST_MINECART.id
        || entity_type.id == EntityType::COMMAND_BLOCK_MINECART.id
        || entity_type.id == EntityType::FURNACE_MINECART.id
        || entity_type.id == EntityType::HOPPER_MINECART.id
        || entity_type.id == EntityType::SPAWNER_MINECART.id
        || entity_type.id == EntityType::TNT_MINECART.id
}

pub struct MinecartEntity {
    pub vehicle: VehicleEntity,
    kind: MinecartKind,
    /// Vanilla `flipped` (`FlippedRotation`): the cart faces against its yaw.
    flipped: AtomicBool,
}

enum MinecartKind {
    Rideable(RideableMinecart),
    Chest(ChestMinecart),
    CommandBlock(CommandBlockMinecart),
    Furnace(FurnaceMinecart),
    Hopper(HopperMinecart),
    Spawner(SpawnerMinecart),
    Tnt(TntMinecart),
    Other,
}

impl MinecartEntity {
    pub fn new(mut entity: Entity) -> Self {
        entity.blocks_building = true;
        let kind = match entity.entity_type.id {
            id if id == EntityType::MINECART.id => MinecartKind::Rideable(RideableMinecart),
            id if id == EntityType::CHEST_MINECART.id => MinecartKind::Chest(ChestMinecart::new()),
            id if id == EntityType::COMMAND_BLOCK_MINECART.id => {
                MinecartKind::CommandBlock(CommandBlockMinecart::new())
            }
            id if id == EntityType::FURNACE_MINECART.id => {
                MinecartKind::Furnace(FurnaceMinecart::new())
            }
            id if id == EntityType::HOPPER_MINECART.id => {
                MinecartKind::Hopper(HopperMinecart::new())
            }
            id if id == EntityType::SPAWNER_MINECART.id => {
                MinecartKind::Spawner(SpawnerMinecart::new())
            }
            id if id == EntityType::TNT_MINECART.id => MinecartKind::Tnt(TntMinecart::new()),
            _ => MinecartKind::Other,
        };
        Self {
            vehicle: VehicleEntity::new(entity),
            kind,
            flipped: AtomicBool::new(false),
        }
    }

    #[must_use]
    pub const fn command_block(&self) -> Option<&CommandBlockMinecart> {
        match &self.kind {
            MinecartKind::CommandBlock(minecart) => Some(minecart),
            _ => None,
        }
    }

    const fn container(&self) -> Option<&Arc<MinecartInventory>> {
        match &self.kind {
            MinecartKind::Chest(minecart) => Some(minecart.inventory()),
            MinecartKind::Hopper(minecart) => Some(minecart.inventory()),
            _ => None,
        }
    }

    const fn drop_item(&self) -> &'static Item {
        match &self.kind {
            MinecartKind::Chest(_) => &Item::CHEST_MINECART,
            MinecartKind::Furnace(_) => &Item::FURNACE_MINECART,
            MinecartKind::Hopper(_) => &Item::HOPPER_MINECART,
            MinecartKind::Tnt(_) => &Item::TNT_MINECART,
            MinecartKind::Rideable(_)
            | MinecartKind::CommandBlock(_)
            | MinecartKind::Spawner(_)
            | MinecartKind::Other => &Item::MINECART,
        }
    }

    /// Vanilla `AbstractMinecart.getMaxSpeed` (`OldMinecartBehavior`, furnace override).
    fn max_speed(&self) -> f64 {
        let in_water = self.vehicle.entity.is_in_water();
        let base = if in_water { 0.2 } else { 0.4 };
        if matches!(self.kind, MinecartKind::Furnace(_)) {
            base * if in_water { 0.75 } else { 0.5 }
        } else {
            base
        }
    }

    /// Vanilla `applyNaturalSlowdown` for each minecart kind.
    fn apply_natural_slowdown(&self, velocity: Vector3<f64>) -> Vector3<f64> {
        let entity = &self.vehicle.entity;
        match &self.kind {
            MinecartKind::Furnace(minecart) => {
                minecart.velocity(entity, velocity, entity.has_passengers())
            }
            _ => self.container().map_or_else(
                || Self::base_slowdown(entity, velocity),
                |inventory| container::velocity(entity, inventory, velocity),
            ),
        }
    }

    /// Vanilla `AbstractMinecart.applyNaturalSlowdown`.
    pub(super) fn base_slowdown(entity: &Entity, velocity: Vector3<f64>) -> Vector3<f64> {
        let factor = if entity.has_passengers() { 0.997 } else { 0.96 };
        let slowed = velocity.multiply(factor, 0.0, factor);
        if entity.is_in_water() {
            slowed.multiply(0.95, 0.95, 0.95)
        } else {
            slowed
        }
    }

    /// Vanilla `AbstractMinecart.getCurrentBlockPosOrRailBelow` (old movement).
    fn current_block_pos_or_rail_below(&self) -> BlockPos {
        let pos = self.vehicle.entity.pos.load();
        let x = pos.x.floor() as i32;
        let mut y = pos.y.floor() as i32;
        let z = pos.z.floor() as i32;
        let world = self.vehicle.entity.world.load();
        if is_rail(world.get_block(&BlockPos::new(x, y - 1, z))) {
            y -= 1;
        }
        BlockPos::new(x, y, z)
    }

    /// Vanilla `OldMinecartBehavior.getPos`: the point on the rail at (x, z), `None` off rails.
    fn rail_pos(&self, x: f64, y: f64, z: f64) -> Option<Vector3<f64>> {
        let world = self.vehicle.entity.world.load();
        let bx = x.floor() as i32;
        let mut by = y.floor() as i32;
        let bz = z.floor() as i32;
        if is_rail(world.get_block(&BlockPos::new(bx, by - 1, bz))) {
            by -= 1;
        }
        let (block, state_id) = world.get_block_and_state_id(&BlockPos::new(bx, by, bz));
        if !is_rail(block) {
            return None;
        }
        let (e0, e1) = get_exits(rail_shape(block, state_id));
        let x0 = f64::from(bx) + 0.5 + f64::from(e0[0]) * 0.5;
        let y0 = f64::from(by) + 0.0625 + f64::from(e0[1]) * 0.5;
        let z0 = f64::from(bz) + 0.5 + f64::from(e0[2]) * 0.5;
        let x1 = f64::from(bx) + 0.5 + f64::from(e1[0]) * 0.5;
        let y1 = f64::from(by) + 0.0625 + f64::from(e1[1]) * 0.5;
        let z1 = f64::from(bz) + 0.5 + f64::from(e1[2]) * 0.5;
        let dx = x1 - x0;
        let dy = (y1 - y0) * 2.0;
        let dz = z1 - z0;
        let t = if dx == 0.0 {
            z - f64::from(bz)
        } else if dz == 0.0 {
            x - f64::from(bx)
        } else {
            ((x - x0) * dx + (z - z0) * dz) * 2.0
        };
        let mut y = y0 + dy * t;
        if dy < 0.0 {
            y += 1.0;
        } else if dy > 0.0 {
            y += 0.5;
        }
        Some(Vector3::new(x0 + dx * t, y, z0 + dz * t))
    }

    /// Vanilla `AbstractMinecart.move` for the old movement: move, then block effects.
    fn move_cart(&self, caller: &dyn EntityBase, motion: Vector3<f64>) {
        let entity = &self.vehicle.entity;
        entity.move_entity(caller, motion);
        entity.tick_block_collisions(caller);
    }

    /// Vanilla `OldMinecartBehavior.tick` on the server.
    fn behavior_tick(&self, caller: &dyn EntityBase, old_yaw: f32) {
        let entity = &self.vehicle.entity;
        let world = entity.world.load();
        // `applyGravity`
        if !entity.has_no_gravity() {
            let gravity = if entity.is_in_water() { 0.005 } else { 0.04 };
            let velocity = entity.velocity.load();
            entity
                .velocity
                .store(Vector3::new(velocity.x, velocity.y - gravity, velocity.z));
        }
        let rail_pos = self.current_block_pos_or_rail_below();
        let (block, state_id) = world.get_block_and_state_id(&rail_pos);
        if is_rail(block) {
            self.move_along_track(caller, rail_pos, block, state_id);
            if block == &Block::ACTIVATOR_RAIL {
                self.activate_minecart(PoweredRailLikeProperties::from_state_id(state_id).powered);
            }
        } else {
            self.come_off_track(caller);
        }
        if entity.is_removed() {
            return;
        }
        // `applyEffectsFromBlocks`
        entity.tick_block_collisions(caller);
        if entity.is_removed() {
            return;
        }

        entity.pitch.store(0.0);
        let last = entity.last_pos.load();
        let pos = entity.pos.load();
        let dx = last.x - pos.x;
        let dz = last.z - pos.z;
        let mut yaw = entity.yaw.load();
        if dx * dx + dz * dz > 0.001 {
            yaw = (dz.atan2(dx) * 180.0 / std::f64::consts::PI) as f32;
            if self.flipped.load(Ordering::Relaxed) {
                yaw += 180.0;
            }
        }
        let turned = f64::from(pumpkin_util::math::wrap_degrees(yaw - old_yaw));
        if !(-170.0..170.0).contains(&turned) {
            yaw += 180.0;
            self.flipped.fetch_xor(true, Ordering::Relaxed);
        }
        entity.yaw.store(yaw % 360.0);

        self.push_and_pickup_entities();
    }

    /// Vanilla `AbstractMinecart.comeOffTrack`.
    fn come_off_track(&self, caller: &dyn EntityBase) {
        let entity = &self.vehicle.entity;
        let max = self.max_speed();
        let velocity = entity.velocity.load();
        let mut velocity = Vector3::new(
            velocity.x.clamp(-max, max),
            velocity.y,
            velocity.z.clamp(-max, max),
        );
        if entity.on_ground.load(Ordering::Relaxed) {
            velocity = velocity.multiply(0.5, 0.5, 0.5);
        }
        entity.velocity.store(velocity);
        self.move_cart(caller, velocity);
        if !entity.on_ground.load(Ordering::Relaxed) {
            let air_drag = f64::from(0.95f32);
            entity.velocity.store(
                entity
                    .velocity
                    .load()
                    .multiply(air_drag, air_drag, air_drag),
            );
        }
    }

    /// Vanilla `OldMinecartBehavior.moveAlongTrack`.
    #[allow(clippy::too_many_lines)]
    fn move_along_track(
        &self,
        caller: &dyn EntityBase,
        rail: BlockPos,
        block: &'static Block,
        state_id: BlockStateId,
    ) {
        let entity = &self.vehicle.entity;
        let world = entity.world.load();
        let pos = entity.pos.load();
        let (mut x, mut z) = (pos.x, pos.z);
        let old_rail_pos = self.rail_pos(pos.x, pos.y, pos.z);
        let mut y = f64::from(rail.0.y);
        let powered = block == &Block::POWERED_RAIL
            && PoweredRailLikeProperties::from_state_id(state_id).powered;
        let mut slow_down = block == &Block::POWERED_RAIL && !powered;
        let mut slope = 0.0078125;
        if entity.is_in_water() {
            slope *= 0.2;
        }
        let shape = rail_shape(block, state_id);
        let mut velocity = entity.velocity.load();
        match shape {
            RailShape::AscendingEast => {
                velocity.x -= slope;
                y += 1.0;
            }
            RailShape::AscendingWest => {
                velocity.x += slope;
                y += 1.0;
            }
            RailShape::AscendingNorth => {
                velocity.z += slope;
                y += 1.0;
            }
            RailShape::AscendingSouth => {
                velocity.z -= slope;
                y += 1.0;
            }
            _ => {}
        }
        let (e0, e1) = get_exits(shape);
        let mut dx = f64::from(e1[0] - e0[0]);
        let mut dz = f64::from(e1[2] - e0[2]);
        let length = dx.hypot(dz);
        if velocity.x * dx + velocity.z * dz < 0.0 {
            dx = -dx;
            dz = -dz;
        }
        let speed = 2.0f64.min(velocity.horizontal_length());
        velocity = Vector3::new(speed * dx / length, velocity.y, speed * dz / length);

        // A riding player only nudges a nearly stopped cart.
        let rider_input = entity
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first()
            .and_then(|passenger| {
                passenger.get_player().map(|player| {
                    (
                        player.last_input.load(Ordering::Relaxed),
                        player.get_entity().yaw.load(),
                    )
                })
            });
        if let Some((input, yaw)) = rider_input {
            let intent = rider_move_intent(input, yaw);
            if intent.length_squared() > 0.0 && velocity.horizontal_length_squared() < 0.01 {
                velocity.x += intent.x * 0.001;
                velocity.z += intent.z * 0.001;
                slow_down = false;
            }
        }
        if slow_down {
            if velocity.horizontal_length() < 0.03 {
                velocity = Vector3::new(0.0, 0.0, 0.0);
            } else {
                velocity = velocity.multiply(0.5, 0.0, 0.5);
            }
        }

        let x0 = f64::from(rail.0.x) + 0.5 + f64::from(e0[0]) * 0.5;
        let z0 = f64::from(rail.0.z) + 0.5 + f64::from(e0[2]) * 0.5;
        let x1 = f64::from(rail.0.x) + 0.5 + f64::from(e1[0]) * 0.5;
        let z1 = f64::from(rail.0.z) + 0.5 + f64::from(e1[2]) * 0.5;
        let dx = x1 - x0;
        let dz = z1 - z0;
        let t = if dx == 0.0 {
            z - f64::from(rail.0.z)
        } else if dz == 0.0 {
            x - f64::from(rail.0.x)
        } else {
            ((x - x0) * dx + (z - z0) * dz) * 2.0
        };
        x = x0 + dx * t;
        z = z0 + dz * t;
        entity.set_pos(Vector3::new(x, y, z));
        entity.velocity.store(velocity);

        let scale = if entity.has_passengers() { 0.75 } else { 1.0 };
        let max = self.max_speed();
        self.move_cart(
            caller,
            Vector3::new(
                (scale * velocity.x).clamp(-max, max),
                0.0,
                (scale * velocity.z).clamp(-max, max),
            ),
        );
        if entity.is_removed() {
            return;
        }

        // Stepping off the low end of a slope drops the cart onto the next block.
        let moved = entity.pos.load();
        let column_x = moved.x.floor() as i32 - rail.0.x;
        let column_z = moved.z.floor() as i32 - rail.0.z;
        if e0[1] != 0 && column_x == e0[0] && column_z == e0[2] {
            entity.set_pos(Vector3::new(moved.x, moved.y + f64::from(e0[1]), moved.z));
        } else if e1[1] != 0 && column_x == e1[0] && column_z == e1[2] {
            entity.set_pos(Vector3::new(moved.x, moved.y + f64::from(e1[1]), moved.z));
        }

        let mut velocity = self.apply_natural_slowdown(entity.velocity.load());
        let moved = entity.pos.load();
        if let Some(new_rail_pos) = self.rail_pos(moved.x, moved.y, moved.z)
            && let Some(old_rail_pos) = old_rail_pos
        {
            let energy = (old_rail_pos.y - new_rail_pos.y) * 0.05;
            let horizontal = velocity.horizontal_length();
            if horizontal > 0.0 {
                let factor = (horizontal + energy) / horizontal;
                velocity = velocity.multiply(factor, 1.0, factor);
            }
            entity.set_pos(Vector3::new(moved.x, new_rail_pos.y, moved.z));
        }

        let moved = entity.pos.load();
        let block_x = moved.x.floor() as i32;
        let block_z = moved.z.floor() as i32;
        if block_x != rail.0.x || block_z != rail.0.z {
            let horizontal = velocity.horizontal_length();
            velocity = Vector3::new(
                horizontal * f64::from(block_x - rail.0.x),
                velocity.y,
                horizontal * f64::from(block_z - rail.0.z),
            );
        }

        if powered {
            let horizontal = velocity.horizontal_length();
            if horizontal > 0.01 {
                velocity.x += velocity.x / horizontal * 0.06;
                velocity.z += velocity.z / horizontal * 0.06;
            } else {
                // A stopped cart is kicked away from a solid block at one end.
                let conductor = |pos: BlockPos| world.get_block_state(&pos).is_solid_block();
                match shape {
                    RailShape::EastWest => {
                        if conductor(rail.west()) {
                            velocity.x = 0.02;
                        } else if conductor(rail.east()) {
                            velocity.x = -0.02;
                        }
                    }
                    RailShape::NorthSouth => {
                        if conductor(rail.north()) {
                            velocity.z = 0.02;
                        } else if conductor(rail.south()) {
                            velocity.z = -0.02;
                        }
                    }
                    _ => {}
                }
            }
        }
        entity.velocity.store(velocity);
    }

    /// Vanilla `activateMinecart` for each kind, run on an activator rail.
    fn activate_minecart(&self, powered: bool) {
        let entity = &self.vehicle.entity;
        match &self.kind {
            MinecartKind::Hopper(minecart) => minecart.set_enabled(!powered),
            MinecartKind::CommandBlock(minecart) if powered => minecart.activate(entity),
            MinecartKind::Tnt(minecart) if powered => minecart.prime(entity, 80),
            MinecartKind::Rideable(_) if powered => {
                let passenger_ids: Vec<i32> = entity
                    .passengers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .iter()
                    .map(|passenger| passenger.get_entity().entity_id)
                    .collect();
                for passenger_id in passenger_ids {
                    entity.remove_passenger_sync(passenger_id);
                }
                if self.vehicle.get_hurt_time() == 0 {
                    self.vehicle.set_hurt_dir(-self.vehicle.get_hurt_dir());
                    self.vehicle.set_hurt_time(10);
                    self.vehicle.set_damage(50.0);
                    self.vehicle.send_wobble_metadata();
                }
            }
            _ => {}
        }
    }

    /// Vanilla `OldMinecartBehavior.pushAndPickupEntities`.
    fn push_and_pickup_entities(&self) {
        let entity = &self.vehicle.entity;
        let world = entity.world.load();
        let area = entity.bounding_box.load().expand(0.2, 0.0, 0.2);
        if matches!(self.kind, MinecartKind::Rideable(_))
            && entity.velocity.load().horizontal_length_squared() >= 0.01
        {
            let team = self.get_team();
            let others: Vec<Arc<dyn EntityBase>> = world
                .get_all_at_box(&area)
                .into_iter()
                .filter(|other| {
                    other.get_entity().entity_id != entity.entity_id
                        && crate::entity::is_pushable_by(team.as_ref(), other.as_ref())
                })
                .collect();
            for other in others {
                let other_type = other.get_entity().entity_type;
                if other.get_player().is_some()
                    || other_type == &EntityType::IRON_GOLEM
                    || is_minecart(other_type)
                    || entity.has_passengers()
                    || other.is_passenger()
                {
                    other.push(self);
                } else if let Some(me) = world.get_entity_by_id(entity.entity_id) {
                    // `Entity.canAddPassenger`: only an empty cart takes a rider.
                    let can_add = !entity.has_passengers();
                    crate::entity::start_riding(&other, &me, can_add);
                }
            }
        } else {
            for other in world.get_entities_at_box(&area) {
                if other.get_entity().entity_id != entity.entity_id
                    && !self.has_passenger(other.as_ref())
                    && other.is_pushable()
                    && is_minecart(other.get_entity().entity_type)
                {
                    other.push(self);
                }
            }
        }
    }
}

const LOWERED_PASSENGER_ATTACHMENT: Vector3<f64> = Vector3::new(0.0, 0.0, 0.0);

impl EntityBase for MinecartEntity {
    fn passenger_attachment_point(
        &self,
        passenger: &dyn EntityBase,
        _index: usize,
    ) -> Option<Vector3<f64>> {
        let passenger_type = passenger.get_entity().entity_type;
        (passenger_type == &EntityType::VILLAGER || passenger_type == &EntityType::WANDERING_TRADER)
            .then_some(LOWERED_PASSENGER_ATTACHMENT)
    }

    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("FlippedRotation", self.flipped.load(Ordering::Relaxed));
        match &self.kind {
            MinecartKind::Chest(minecart) => minecart.write_nbt(nbt),
            MinecartKind::CommandBlock(minecart) => minecart.write_nbt(nbt),
            MinecartKind::Furnace(minecart) => minecart.write_nbt(nbt),
            MinecartKind::Hopper(minecart) => minecart.write_nbt(nbt),
            MinecartKind::Spawner(minecart) => minecart.write_nbt(nbt),
            MinecartKind::Tnt(minecart) => minecart.write_nbt(nbt),
            MinecartKind::Rideable(_) | MinecartKind::Other => {}
        }
    }

    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        if let Some(flipped) = nbt.get_bool("FlippedRotation") {
            self.flipped.store(flipped, Ordering::Relaxed);
        }
        match &self.kind {
            MinecartKind::Chest(minecart) => minecart.read_nbt(nbt),
            MinecartKind::CommandBlock(minecart) => minecart.read_nbt(&self.vehicle.entity, nbt),
            MinecartKind::Furnace(minecart) => minecart.read_nbt(nbt),
            MinecartKind::Hopper(minecart) => minecart.read_nbt(nbt),
            MinecartKind::Spawner(minecart) => minecart.read_nbt(nbt),
            MinecartKind::Tnt(minecart) => minecart.read_nbt(nbt),
            MinecartKind::Rideable(_) | MinecartKind::Other => {}
        }
    }

    /// Vanilla `AbstractMinecart.tick` with `OldMinecartBehavior.tick`.
    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        let entity = &self.vehicle.entity;
        entity.update_last_pos();
        let old_yaw = entity.yaw.load();
        // Hurt time / damage decay.
        self.vehicle.tick();
        if let MinecartKind::CommandBlock(minecart) = &self.kind {
            minecart.step_tick();
        }
        // `checkBelowWorld`, `handlePortal`.
        entity.check_out_of_world(caller);
        if entity.is_removed() {
            return;
        }
        entity.tick_portal(caller);
        if entity.is_removed() {
            return;
        }

        self.behavior_tick(caller, old_yaw);
        if entity.is_removed() {
            return;
        }

        // `updateFluidInteraction`, then the minecart's own lava handling.
        entity.update_fluid_state(caller);
        if entity.touching_lava.load(Ordering::SeqCst) && !entity.is_fire_immune() {
            if let Some(seconds) =
                crate::entity::combust_by_block(entity, 15.0, entity.block_pos.load())
            {
                self.set_on_fire_for_cause(
                    seconds,
                    crate::plugin::api::events::entity::entity_combust::CombustCause::Lava,
                );
            }
            entity.damage(self, 4.0, DamageType::LAVA);
            if entity.is_removed() {
                return;
            }
        }
        entity.first_tick.store(false, Ordering::Relaxed);

        match &self.kind {
            MinecartKind::Furnace(minecart) => minecart.tick(entity),
            MinecartKind::Hopper(minecart) => minecart.tick(entity),
            MinecartKind::Spawner(minecart) => minecart.tick(entity),
            MinecartKind::Tnt(minecart) => {
                if minecart.tick(self) {
                    return;
                }
                // `MinecartTNT.tick`: running into a wall fast enough sets it off.
                let speed_squared = entity.velocity.load().horizontal_length_squared();
                if entity.horizontal_collision.load(Ordering::Relaxed)
                    && speed_squared >= f64::from(0.01f32)
                {
                    minecart.explode(self, speed_squared, None);
                }
            }
            _ => {}
        }
    }

    fn get_entity(&self) -> &Entity {
        &self.vehicle.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn has_container_inventory(&self) -> bool {
        self.container().is_some()
    }

    fn get_container_inventory(&self) -> Option<Arc<dyn Inventory>> {
        let container = self.container()?;
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

    fn is_pushable(&self) -> bool {
        true
    }

    fn collides_with_pushables(&self) -> bool {
        true
    }

    #[allow(clippy::too_many_lines)]
    fn push(&self, entity: &dyn EntityBase) {
        let self_entity = self.get_entity();
        let other_entity = entity.get_entity();

        if self_entity.no_physics.load(Ordering::Relaxed)
            || other_entity.no_physics.load(Ordering::Relaxed)
        {
            return;
        }

        {
            let passengers = self_entity
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if passengers
                .iter()
                .any(|p| p.get_entity().entity_id == other_entity.entity_id)
            {
                return;
            }
        }
        {
            let passengers = other_entity
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if passengers
                .iter()
                .any(|p| p.get_entity().entity_id == self_entity.entity_id)
            {
                return;
            }
        }

        let mut xa = other_entity.pos.load().x - self_entity.pos.load().x;
        let mut za = other_entity.pos.load().z - self_entity.pos.load().z;
        let mut dd = xa * xa + za * za;
        if dd >= 1.0E-4 {
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let mut pow = 1.0 / dd;
            if pow > 1.0 {
                pow = 1.0;
            }
            xa *= pow;
            za *= pow;
            xa *= 0.1;
            za *= 0.1;
            xa *= 0.5;
            za *= 0.5;

            let is_other_minecart = other_entity.entity_type.id == EntityType::MINECART.id
                || other_entity.entity_type.id == EntityType::CHEST_MINECART.id
                || other_entity.entity_type.id == EntityType::COMMAND_BLOCK_MINECART.id
                || other_entity.entity_type.id == EntityType::FURNACE_MINECART.id
                || other_entity.entity_type.id == EntityType::HOPPER_MINECART.id
                || other_entity.entity_type.id == EntityType::SPAWNER_MINECART.id
                || other_entity.entity_type.id == EntityType::TNT_MINECART.id;

            if is_other_minecart {
                let xo = self_entity.velocity.load().x;
                let zo = self_entity.velocity.load().z;

                let dir = Vector3::new(xo, 0.0, zo).normalize();
                let facing = Vector3::new(
                    f64::from(self_entity.yaw.load().to_radians().cos()),
                    0.0,
                    f64::from(self_entity.yaw.load().to_radians().sin()),
                )
                .normalize();

                let dot = dir.dot(&facing).abs();
                if dot >= 0.8 {
                    let vel = self_entity.velocity.load();
                    let ovel = other_entity.velocity.load();

                    let is_self_furnace =
                        self_entity.entity_type.id == EntityType::FURNACE_MINECART.id;
                    let is_other_furnace =
                        other_entity.entity_type.id == EntityType::FURNACE_MINECART.id;

                    if is_other_furnace && !is_self_furnace {
                        self_entity.velocity.store(vel.multiply(0.2, 1.0, 0.2));
                        let mut new_self_vel = self_entity.velocity.load();
                        new_self_vel.x += ovel.x - xa;
                        new_self_vel.z += ovel.z - za;
                        self_entity.velocity.store(new_self_vel);
                        self_entity.send_velocity();

                        other_entity.velocity.store(ovel.multiply(0.95, 1.0, 0.95));
                        other_entity.send_velocity();
                    } else if !is_other_furnace && is_self_furnace {
                        other_entity.velocity.store(ovel.multiply(0.2, 1.0, 0.2));
                        let mut new_other_vel = other_entity.velocity.load();
                        new_other_vel.x += vel.x + xa;
                        new_other_vel.z += vel.z + za;
                        other_entity.velocity.store(new_other_vel);
                        other_entity.send_velocity();

                        self_entity.velocity.store(vel.multiply(0.95, 1.0, 0.95));
                        self_entity.send_velocity();
                    } else {
                        #[allow(clippy::manual_midpoint)]
                        let xdd = (ovel.x + vel.x) / 2.0;
                        #[allow(clippy::manual_midpoint)]
                        let zdd = (ovel.z + vel.z) / 2.0;

                        self_entity.velocity.store(vel.multiply(0.2, 1.0, 0.2));
                        let mut new_self_vel = self_entity.velocity.load();
                        new_self_vel.x += xdd - xa;
                        new_self_vel.z += zdd - za;
                        self_entity.velocity.store(new_self_vel);
                        self_entity.send_velocity();

                        other_entity.velocity.store(ovel.multiply(0.2, 1.0, 0.2));
                        let mut new_other_vel = other_entity.velocity.load();
                        new_other_vel.x += xdd + xa;
                        new_other_vel.z += zdd + za;
                        other_entity.velocity.store(new_other_vel);
                        other_entity.send_velocity();
                    }
                }
            } else {
                // Vanilla `AbstractMinecart.push`
                self_entity.push_by(Vector3::new(-xa, 0.0, -za));
                other_entity.push_by(Vector3::new(xa / 4.0, 0.0, za / 4.0));
            }
        }
    }

    fn is_collidable(&self, _entity: Option<Box<dyn EntityBase>>) -> bool {
        true
    }

    fn init_data_tracker(&self) {
        self.vehicle.send_wobble_metadata();
        match &self.kind {
            MinecartKind::CommandBlock(minecart) => {
                minecart.init_data_tracker(&self.vehicle.entity);
            }
            MinecartKind::Furnace(minecart) => {
                minecart.init_data_tracker(&self.vehicle.entity);
            }
            _ => {}
        }
    }

    fn can_hit(&self) -> bool {
        self.vehicle.entity.is_alive()
    }

    fn damage_with_context(
        &self,
        _caller: &dyn EntityBase,
        amount: f32,
        damage_type: DamageType,
        _position: Option<Vector3<f64>>,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        let creative = source
            .and_then(EntityBase::get_player)
            .is_some_and(|player| player.gamemode.load() == GameMode::Creative);

        if let MinecartKind::Tnt(minecart) = &self.kind
            && let Some(projectile) = source
            && is_abstract_arrow(projectile.get_entity().entity_type)
            && projectile.get_entity().is_on_fire()
        {
            let world = self.vehicle.entity.world.load();
            let cause =
                cause.and_then(|cause| world.get_entity_by_id(cause.get_entity().entity_id));
            minecart.explode(
                self,
                projectile.get_entity().velocity.load().length_squared(),
                cause,
            );
            if self.vehicle.entity.is_removed() {
                return true;
            }
        }

        let will_break = self.vehicle.entity.is_alive()
            && (creative || self.vehicle.get_damage() + amount * 10.0 > 40.0);

        if let MinecartKind::Tnt(minecart) = &self.kind
            && will_break
            && !creative
        {
            let velocity = self.vehicle.entity.velocity.load();
            let speed_squared = velocity.x.mul_add(velocity.x, velocity.z * velocity.z);
            let ignites = damage_type.has_tag(&tag::DamageType::MINECRAFT_IS_FIRE)
                || damage_type.has_tag(&tag::DamageType::MINECRAFT_IS_EXPLOSION)
                || self.vehicle.entity.fire_ticks.load(Ordering::Relaxed) > 0;
            if ignites || speed_squared >= 0.01 {
                self.vehicle.apply_damage_wobble(amount);
                let fuse = rand::rng().random_range(0..20) + rand::rng().random_range(0..20);
                if self
                    .vehicle
                    .entity
                    .world
                    .load()
                    .level_info
                    .load()
                    .game_rules
                    .tnt_explodes
                {
                    minecart.prime(&self.vehicle.entity, fuse);
                } else {
                    minecart.set_fuse(fuse);
                }
                return true;
            }
        }

        let damaged = self.vehicle.damage_with_context(amount, source);

        if will_break && !creative && self.vehicle.entity.is_removed() {
            let world = self.vehicle.entity.world.load();
            if world.level_info.load().game_rules.entity_drops {
                let position = self.vehicle.entity.block_pos.load();
                if let Some(container) = self.container()
                    && container.claim_drops()
                {
                    container.unpack_loot(&crate::world::loot::LootContextParameters {
                        world: Some(&world),
                        position: Some(self.vehicle.entity.pos.load()),
                        ..Default::default()
                    });
                    let inventory: Arc<dyn Inventory> = container.clone();
                    world.scatter_inventory(&position, &inventory);
                }
                let mut stack = ItemStack::new(1, self.drop_item());
                if let Some(custom_name) = self.vehicle.entity.custom_name.load().as_ref() {
                    stack.set_data_component(pumpkin_data::data_component_impl::CustomNameImpl {
                        name: custom_name.clone(),
                    });
                }
                world.drop_stack(&position, stack);
            }
            // Vanilla `ContainerEntity.chestVehicleDestroyed`: a player breaking it angers piglins
            if self.container().is_some()
                && let Some(player) = source
                    .and_then(EntityBase::get_player)
                    .and_then(|player| world.get_player_by_uuid(player.gameprofile.id))
            {
                crate::entity::mob::piglin_ai::anger_nearby_piglins(&world, &player, true);
            }
        }

        damaged
    }

    fn interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        match &self.kind {
            MinecartKind::Chest(minecart) => {
                minecart.interact(&self.vehicle.entity, player);
                true
            }
            MinecartKind::CommandBlock(_) => CommandBlockMinecart::interact(player),
            MinecartKind::Furnace(minecart) => {
                minecart.interact(&self.vehicle.entity, player, item_stack)
            }
            MinecartKind::Hopper(minecart) => {
                minecart.interact(&self.vehicle.entity, player);
                true
            }
            MinecartKind::Rideable(_) => RideableMinecart::interact(&self.vehicle.entity, player),
            MinecartKind::Spawner(_) | MinecartKind::Tnt(_) | MinecartKind::Other => false,
        }
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::ai::test_support::world;
    use pumpkin_util::text::TextComponent;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[tokio::test]
    async fn command_block_minecart_nbt_and_activation_cooldown() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let cart = MinecartEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::COMMAND_BLOCK_MINECART,
        ));

        let mut nbt = NbtCompound::new();
        nbt.put_string("Command", "Searge".to_string());
        nbt.put_bool("TrackOutput", true);
        nbt.put_long("LastExecution", 12);
        cart.read_nbt_non_mut(&nbt);

        let Some(cb) = cart.command_block() else {
            return Err("expected CommandBlockMinecart".into());
        };
        assert_eq!(cb.get_command(), "Searge");
        assert_eq!(cb.get_success_count(), 0);

        // Fewer than 4 ticks elapsed since `last_activated` (0): must not activate.
        for _ in 0..3 {
            cb.step_tick();
            cb.activate(&cart.vehicle.entity);
        }
        assert_eq!(cb.get_success_count(), 0);

        // Tick 4 reaches ACTIVATION_DELAY (4) and triggers the command.
        cb.step_tick();
        cb.activate(&cart.vehicle.entity);
        assert_eq!(cb.get_success_count(), 1);
        assert_eq!(
            cb.get_last_output_or_empty(),
            TextComponent::text("#itzlipofutzli")
        );

        let mut saved = NbtCompound::new();
        cart.write_nbt(&mut saved);
        assert_eq!(saved.get_string("Command"), Some("Searge"));
        assert_eq!(saved.get_int("SuccessCount"), Some(1));
        assert_eq!(saved.get_bool("TrackOutput"), Some(true));
        assert!(saved.get("LastOutput").is_some());

        // Setting TrackOutput=false clears LastOutput on load and omits it on save.
        let mut no_track = saved.clone();
        no_track.put_bool("TrackOutput", false);
        cart.read_nbt_non_mut(&no_track);
        let mut saved_no_track = NbtCompound::new();
        cart.write_nbt(&mut saved_no_track);
        assert!(saved_no_track.get("LastOutput").is_none());

        world.level.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn spawner_minecart_nbt_roundtrip() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let cart = MinecartEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::SPAWNER_MINECART,
        ));

        let mut spawn_entity = NbtCompound::new();
        spawn_entity.put_string("id", "minecraft:pig".to_string());
        let mut spawn_data = NbtCompound::new();
        spawn_data.put_compound("entity", spawn_entity);

        let mut nbt = NbtCompound::new();
        nbt.put_short("Delay", 45);
        nbt.put_short("MinSpawnDelay", 100);
        nbt.put_short("MaxSpawnDelay", 300);
        nbt.put_short("SpawnCount", 2);
        nbt.put_short("SpawnRange", 3);
        nbt.put_short("MaxNearbyEntities", 5);
        nbt.put_short("RequiredPlayerRange", 12);
        nbt.put_compound("SpawnData", spawn_data);

        cart.read_nbt_non_mut(&nbt);

        let mut saved = NbtCompound::new();
        cart.write_nbt(&mut saved);
        assert_eq!(saved.get_short("Delay"), Some(45));
        assert_eq!(saved.get_short("MinSpawnDelay"), Some(100));
        assert_eq!(saved.get_short("MaxSpawnDelay"), Some(300));
        assert_eq!(saved.get_short("SpawnCount"), Some(2));
        assert_eq!(saved.get_short("SpawnRange"), Some(3));
        assert_eq!(saved.get_short("MaxNearbyEntities"), Some(5));
        assert_eq!(saved.get_short("RequiredPlayerRange"), Some(12));
        assert_eq!(
            saved
                .get_compound("SpawnData")
                .and_then(|d| d.get_compound("entity"))
                .and_then(|e| e.get_string("id")),
            Some("minecraft:pig")
        );

        world.level.shutdown().await;
        Ok(())
    }
}
