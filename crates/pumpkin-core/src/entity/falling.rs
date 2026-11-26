use crossbeam::atomic::AtomicCell;
use pumpkin_data::Block;
use pumpkin_data::BlockStateId;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::client::CUpdateBlock;
use pumpkin_protocol::java::client::play::CBlockUpdate;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::resource_location::ToResourceLocation;
use pumpkin_world::generation::structure::template::{BlockStateResolver, PaletteEntry};
use pumpkin_world::world::BlockFlags;
use std::sync::atomic::{AtomicBool, AtomicI32};
use std::sync::{Arc, Mutex, atomic::Ordering};

use crate::{
    block::{
        blocks::falling::FallingBlock, entities::block_entity_from_nbt,
        registry::can_replace_with_other_block,
    },
    entity::{Entity, EntityBase, living::LivingEntity},
    plugin::api::events::entity::entity_change_block::EntityChangeBlockEvent,
    server::Server,
    world::World,
};

/// Vanilla `Entity.getAirDrag`.
const AIR_DRAG: f64 = 0.98;
/// Vanilla `FallingBlockEntity.tick`: drop after this many ticks while outside the build height.
const TIME_OUT_OF_WORLD: i32 = 100;
/// Vanilla `FallingBlockEntity.tick`: drop after this many ticks while still falling.
const TIME_MAX: i32 = 600;

fn state_to_nbt(state_id: BlockStateId) -> NbtCompound {
    let block = Block::from_state_id(state_id);
    let properties = block
        .properties(state_id)
        .map(|props| props.to_props())
        .unwrap_or_default();
    PaletteEntry::with_properties(block.to_resource_location(), properties).to_nbt_compound()
}

/// Vanilla falls back to sand when `BlockState` is missing or unknown.
fn state_from_nbt(nbt: &NbtCompound) -> BlockStateId {
    nbt.get_compound("BlockState")
        .and_then(|state| PaletteEntry::from_nbt_compound(state).ok())
        .and_then(|entry| BlockStateResolver::resolve_simple(&entry))
        .map_or(Block::SAND.default_state.id, |state| state.id)
}

pub struct FallingEntity {
    entity: Entity,
    block_state_id: AtomicCell<BlockStateId>,
    time: AtomicI32,
    /// Vanilla `FallingBlockEntity.dropItem`.
    drop_item: AtomicBool,
    /// Vanilla `FallingBlockEntity.cancelDrop`: land as nothing, neither block nor item.
    cancel_drop: AtomicBool,
    /// Vanilla `FallingBlockEntity.blockData`, NBT `TileEntityData`.
    block_data: Mutex<Option<NbtCompound>>,
    /// Vanilla `hurtEntities` with `fallDamagePerDistance` and `fallDamageMax`.
    hurt: Mutex<Option<(f32, i32)>>,
    start_y: AtomicCell<f64>,
}

impl FallingEntity {
    pub const fn new(mut entity: Entity, block_state_id: BlockStateId) -> Self {
        entity.blocks_building = true;
        Self {
            entity,
            block_state_id: AtomicCell::new(block_state_id),
            time: AtomicI32::new(0),
            drop_item: AtomicBool::new(true),
            cancel_drop: AtomicBool::new(false),
            block_data: Mutex::new(None),
            hurt: Mutex::new(None),
            start_y: AtomicCell::new(f64::NAN),
        }
    }

    /// Vanilla `FallingBlockEntity.disableDrop`: land as nothing, neither block nor item.
    pub fn disable_drop(&self) {
        self.cancel_drop.store(true, Ordering::Relaxed);
    }

    /// Vanilla `setHurtsEntities`.
    pub fn set_hurts_entities(&self, damage_per_distance: f32, max_damage: i32) {
        *self
            .hurt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((damage_per_distance, max_damage));
    }

    fn state(&self) -> BlockStateId {
        self.block_state_id.load()
    }

    /// Vanilla `causeFallDamage`: crush whatever the block lands on.
    fn cause_fall_damage(&self, caller: &dyn EntityBase, fall_distance: f64) {
        let Some((per_distance, max)) = *self
            .hurt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        else {
            return;
        };
        #[expect(clippy::cast_possible_truncation, reason = "vanilla ceils to an int")]
        let distance = (fall_distance - 1.0).ceil() as i32;
        if distance < 0 {
            return;
        }
        let state = self.state();
        let block = Block::from_state_id(state);
        let damage_type = if block.has_tag(&tag::Block::MINECRAFT_ANVIL) {
            DamageType::FALLING_ANVIL
        } else if block.has_tag(&tag::Block::MINECRAFT_SPELEOTHEMS) {
            DamageType::FALLING_STALACTITE
        } else {
            DamageType::FALLING_BLOCK
        };
        #[expect(clippy::cast_possible_truncation, reason = "vanilla floors to an int")]
        #[expect(clippy::cast_precision_loss, reason = "small fall distances")]
        let damage = ((distance as f32 * per_distance).floor() as i32).min(max);
        #[expect(clippy::cast_precision_loss, reason = "damage is at most 40")]
        let damage = damage as f32;
        let world = self.entity.world.load();
        for target in world.get_all_at_box(&self.entity.bounding_box.load()) {
            let alive = target
                .get_living_entity()
                .is_some_and(|living| living.entity.is_alive());
            let exempt = target.is_spectator()
                || target.get_player().is_some_and(|player| {
                    player.gamemode.load() == pumpkin_util::GameMode::Creative
                });
            if alive && !exempt {
                target.damage_with_context(
                    target.as_ref(),
                    damage,
                    damage_type,
                    None,
                    Some(caller),
                    None,
                );
            }
        }
        #[expect(clippy::cast_precision_loss, reason = "small fall distances")]
        let chip_chance = (distance as f32).mul_add(0.05, 0.05);
        if block.has_tag(&tag::Block::MINECRAFT_ANVIL)
            && damage > 0.0
            && rand::random::<f32>() < chip_chance
        {
            match crate::block::blocks::anvil::AnvilBlock::damage(state) {
                Some(next) => self.block_state_id.store(next),
                None => self.cancel_drop.store(true, Ordering::Relaxed),
            }
        }
    }

    /// Replaced the current Block and Spawns a new Falling one (synchronous)
    pub fn replace_spawn(
        world: &Arc<World>,
        position: BlockPos,
        block_state: BlockStateId,
    ) -> Option<Arc<Self>> {
        let block = Block::from_state_id(block_state);
        // Vanilla copies the state with `WATERLOGGED` cleared onto the entity.
        // `None` means the state has no water to clear.
        let falling_state = block
            .set_waterlogged(block_state, false)
            .unwrap_or(block_state);
        // Vanilla `FallingBlockEntity.fall`: leave `state.getFluidState().createLegacyBlock()`.
        let leftover = if block.is_waterlogged(block_state) {
            Block::WATER.default_state.id
        } else {
            Block::AIR.default_state.id
        };

        // `block_state` can differ from the world's (scaffolding passes its recomputed state), so the
        // write below compares against the cell as it was before the event.
        let observed = world.get_block_state_id(&position);
        let spawn_pos = position.0.to_f64().add_raw(0.5, 0.0, 0.5);
        let entity = Entity::new(world.clone(), spawn_pos, &EntityType::FALLING_BLOCK);
        if world.cancelled_by_plugin(|| {
            EntityChangeBlockEvent::new(
                entity.entity_id,
                position,
                Block::from_state_id(leftover).to_resource_location(),
            )
        }) {
            return None;
        }

        // An event handler may have run long enough for another task to change the source block.
        world.set_block_state_if(&position, leftover, BlockFlags::NOTIFY_ALL, |current| {
            current == observed
        })?;
        entity
            .data
            .store(i32::from(falling_state.as_u16()), Ordering::Relaxed);
        let falling = Arc::new(Self::new(entity, falling_state));
        // Vanilla `AnvilBlock.falling`.
        if Block::from_state_id(falling_state).has_tag(&tag::Block::MINECRAFT_ANVIL) {
            falling.set_hurts_entities(2.0, 40);
        }
        world.spawn_entity_non_save(falling.clone());
        Some(falling)
    }

    /// Vanilla merges `TileEntityData` over the landed block's block entity.
    fn apply_block_data(&self, world: &Arc<World>, pos: BlockPos) {
        let block_data = self
            .block_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(data) = block_data.as_ref() else {
            return;
        };
        let Some(block_entity) = world.get_block_entity(&pos) else {
            return;
        };
        let mut merged = NbtCompound::new();
        block_entity.write_internal(&mut merged);
        for (key, value) in &data.child_tags {
            // Position and id stay those of the landed block entity.
            if !matches!(key.as_ref(), "x" | "y" | "z" | "id") {
                merged.child_tags.insert(key.clone(), value.clone());
            }
        }
        if let Some(reloaded) = block_entity_from_nbt(&merged) {
            world.add_block_entity(reloaded);
        }
    }

    fn drop_as_item(&self) {
        let world = self.entity.world.load();
        let block = Block::from_state_id(self.block_state_id.load());
        // Vanilla: `callOnBrokenAfterFall` then `spawnAtLocation`, both only with drops on.
        if self.drop_item.load(Ordering::Relaxed) && world.level_info.load().game_rules.entity_drops
        {
            self.on_broken_after_fall(&world, block);
            if let Some(item) = Item::from_id(block.item_id) {
                self.entity.spawn_at_location(ItemStack::new(1, item));
            }
        }
    }

    /// Vanilla `FallingBlockEntity.tick` for concrete powder: in water it lands at once; moving
    /// faster than one block per tick, the source water its path crossed is where it lands.
    fn concrete_water_landing(
        &self,
        from: pumpkin_util::math::vector3::Vector3<f64>,
    ) -> (bool, Option<BlockPos>) {
        let block = Block::from_state_id(self.block_state_id.load());
        if FallingBlock::concrete_of(block).is_none() {
            return (false, None);
        }
        let world = self.entity.world.load();
        let pos = self.entity.block_pos.load();
        let mut solidify = world.get_fluid(&pos).has_tag(&tag::Fluid::MINECRAFT_WATER);
        let mut landing = None;
        if self.entity.velocity.load().length_squared() > 1.0 {
            let hit = world.clip(&pumpkin_world::clip::ClipContext::new(
                from,
                self.entity.pos.load(),
                pumpkin_world::clip::ClipBlock::Outline,
                pumpkin_world::clip::ClipFluid::SourceOnly,
            ));
            if !hit.is_miss
                && world
                    .get_fluid(&hit.block_pos)
                    .has_tag(&tag::Fluid::MINECRAFT_WATER)
            {
                landing = Some(hit.block_pos);
                solidify = true;
            }
        }
        (solidify, landing)
    }

    /// After the block is placed: block entity data, `Fallable.onLand` and the block update.
    fn landed(&self, world: &Arc<World>, block: &Block, landing_pos: BlockPos) {
        self.apply_block_data(world, landing_pos);
        // Vanilla `Fallable.onLand`.
        if block.has_tag(&tag::Block::MINECRAFT_ANVIL) && !self.entity.is_silent() {
            world.sync_world_event(
                pumpkin_data::world::WorldEvent::SoundAnvilLand,
                landing_pos,
                0,
            );
        }
        // block updates to watchers before the despawn, else a invisible block gap until the tick flush.
        let placed = world.get_block_state_id(&landing_pos);
        world.send_to_tracking_players_editioned(
            &self.entity,
            &CBlockUpdate::new(landing_pos, i32::from(placed.as_u16()).into()),
            &CUpdateBlock::new(
                landing_pos,
                pumpkin_data::block_remap::bedrock_runtime_id(placed),
            ),
        );
    }

    /// Vanilla `Fallable.onBrokenAfterFall`.
    fn on_broken_after_fall(&self, world: &World, block: &Block) {
        let event = if block.has_tag(&tag::Block::MINECRAFT_ANVIL) {
            Some(pumpkin_data::world::WorldEvent::SoundAnvilBroken)
        } else if block == &Block::POINTED_DRIPSTONE {
            Some(pumpkin_data::world::WorldEvent::SoundPointedDripstoneLand)
        } else if block == &Block::SULFUR_SPIKE {
            Some(pumpkin_data::world::WorldEvent::SoundSulfurSpikeLand)
        } else {
            None
        };
        if let Some(event) = event
            && !self.entity.is_silent()
        {
            world.sync_world_event(event, self.entity.block_pos.load(), 0);
        }
    }
}

impl EntityBase for FallingEntity {
    fn can_hit(&self) -> bool {
        !self.get_entity().is_removed()
    }

    #[expect(clippy::too_many_lines, reason = "one vanilla tick, kept in order")]
    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        let entity = &self.entity;
        if self.block_state_id.load() == BlockStateId::AIR {
            entity.remove();
            return;
        }
        if self.start_y.load().is_nan() {
            self.start_y.store(entity.pos.load().y);
        }
        // `Time` comes from NBT; wraps like vanilla's `time++` instead of overflowing.
        let time = self.time.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let mut velo = entity.velocity.load();
        velo.y -= self.get_gravity();

        entity.velocity.store(velo);

        let from = entity.pos.load();
        entity.move_entity(caller, velo);
        entity.tick_block_collisions(caller);
        // Vanilla `setRequiresPrecisePosition(horizontalCollision || verticalCollision)`.
        entity.requires_precise_position.store(
            entity.horizontal_collision.load(Ordering::Relaxed)
                || entity.vertical_collision.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        // Vanilla: concrete powder lands as soon as it is in water, and a fast one also catches
        // the source water it passed through this tick.
        let (solidify, water_pos) = self.concrete_water_landing(from);
        if entity.on_ground.load(Ordering::Relaxed) || solidify {
            if entity.on_ground.load(Ordering::Relaxed) {
                entity
                    .velocity
                    .store(entity.velocity.load().multiply(0.7, -0.5, 0.7));
            }
            let world = entity.world.load();
            let landing_pos = water_pos.unwrap_or_else(|| self.entity.block_pos.load());
            let (current_block, current_state) = world.get_block_and_state(&landing_pos);
            // Vanilla waits until a piston has finished moving a block through the landing cell.
            if current_block != &Block::MOVING_PISTON {
                self.cause_fall_damage(caller, self.start_y.load() - entity.pos.load().y);
                let mut state_id = self.block_state_id.load();
                let block = Block::from_state_id(state_id);
                let (below_block, below_state) = world.get_block_and_state(&landing_pos.down());
                let may_replace = can_replace_with_other_block(current_block, current_state);
                let would_continue_falling =
                    FallingBlock::can_fall_through(below_state, below_block);
                let would_survive = world.block_registry.can_place_at(
                    None,
                    Some(&**world),
                    &**world,
                    None,
                    block,
                    state_id.to_state(),
                    &landing_pos,
                    None,
                    None,
                ) && !would_continue_falling;
                if self.cancel_drop.load(Ordering::Relaxed) {
                    // Vanilla `cancelDrop`: the entity just disappears.
                    self.on_broken_after_fall(&world, block);
                } else if may_replace && would_survive {
                    if let Some(concrete) = FallingBlock::concrete_of(block)
                        && FallingBlock::should_solidify(&**world, &landing_pos)
                    {
                        state_id = concrete.default_state.id;
                    }
                    let place_block = Block::from_state_id(state_id);
                    // Vanilla waterlogs only in source water (`Fluids.WATER`).
                    if place_block.is_waterloggable()
                        && world.get_fluid(&landing_pos).id == pumpkin_data::fluid::Fluid::WATER.id
                        && world.get_fluid_and_fluid_state(&landing_pos).1.is_source
                        && let Some(waterlogged) = place_block.set_waterlogged(state_id, true)
                    {
                        state_id = waterlogged;
                    }
                    if world.cancelled_by_plugin(|| {
                        EntityChangeBlockEvent::new(
                            entity.entity_id,
                            landing_pos,
                            Block::from_state_id(state_id).to_resource_location(),
                        )
                    }) || world
                        .set_block_state_if(&landing_pos, state_id, BlockFlags::NOTIFY_ALL, |now| {
                            now == current_state.id
                        })
                        .is_none()
                    {
                        // Vanilla drops the block when `setBlock` refuses; here also when the cell changed under the event.
                        self.drop_as_item();
                    } else {
                        self.landed(&world, block, landing_pos);
                    }
                } else {
                    self.drop_as_item();
                }
                self.entity.remove();
            }
        } else {
            let pos = entity.block_pos.load();
            let world = entity.world.load();
            let out_of_world = pos.0.y <= world.get_bottom_y() || pos.0.y > world.get_top_y();
            if (time > TIME_OUT_OF_WORLD && out_of_world) || time > TIME_MAX {
                self.drop_as_item();
                self.entity.remove();
            }
        }

        entity.velocity.store(
            entity
                .velocity
                .load()
                .multiply(AIR_DRAG, AIR_DRAG, AIR_DRAG),
        );
    }

    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_compound("BlockState", state_to_nbt(self.block_state_id.load()));
        nbt.put_int("Time", self.time.load(Ordering::Relaxed));
        nbt.put_bool("DropItem", self.drop_item.load(Ordering::Relaxed));
        nbt.put_bool("CancelDrop", self.cancel_drop.load(Ordering::Relaxed));
        if let Some(data) = self
            .block_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            nbt.put_compound("TileEntityData", data.clone());
        }
    }

    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        let state_id = state_from_nbt(nbt);
        self.block_state_id.store(state_id);
        self.entity
            .data
            .store(i32::from(state_id.as_u16()), Ordering::Relaxed);
        self.time
            .store(nbt.get_numeric_int("Time").unwrap_or(0), Ordering::Relaxed);
        self.drop_item
            .store(nbt.get_bool("DropItem").unwrap_or(true), Ordering::Relaxed);
        self.cancel_drop.store(
            nbt.get_bool("CancelDrop").unwrap_or(false),
            Ordering::Relaxed,
        );
        *self
            .block_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            nbt.get_compound("TileEntityData").cloned();
    }

    fn init_data_tracker(&self) {
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::falling_block::START_POS,
            self.entity.block_pos.load(),
        );
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }
    fn damage(&self, _caller: &dyn EntityBase, _amount: f32, _damage_type: DamageType) -> bool {
        false
    }

    fn get_gravity(&self) -> f64 {
        0.04
    }

    fn entity_bedrock_spawn_metadata(
        &self,
    ) -> Option<pumpkin_protocol::bedrock::client::set_actor_data::SyncedActorDataList> {
        let mut meta =
            pumpkin_protocol::bedrock::client::set_actor_data::SyncedActorDataList::new();
        let runtime_id = pumpkin_data::block_remap::bedrock_runtime_id(self.block_state_id.load());
        meta.set(
            pumpkin_protocol::bedrock::client::set_actor_data::entity_data_key::VARIANT,
            pumpkin_protocol::bedrock::client::set_actor_data::MetadataValue::Int(
                runtime_id as i32,
            ),
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

#[cfg(test)]
mod tests {
    use super::{state_from_nbt, state_to_nbt};
    use pumpkin_data::Block;
    use pumpkin_data::block_properties::Facing;
    use pumpkin_nbt::compound::NbtCompound;

    fn wrapped(state: pumpkin_data::BlockStateId) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_compound("BlockState", state_to_nbt(state));
        nbt
    }

    #[test]
    fn plain_state_round_trips() {
        let state = Block::GRAVEL.default_state.id;
        assert_eq!(state_from_nbt(&wrapped(state)), state);
    }

    #[test]
    fn state_with_properties_round_trips() {
        let state = Block::REPEATER
            .from_properties(&[pumpkin_data::Property::Facing(Facing::South)])
            .to_state_id(&Block::REPEATER);
        assert_eq!(state_from_nbt(&wrapped(state)), state);
    }

    #[test]
    fn missing_or_unknown_state_is_sand() {
        assert_eq!(
            state_from_nbt(&NbtCompound::new()),
            Block::SAND.default_state.id
        );

        let mut unknown = NbtCompound::new();
        let mut entry = NbtCompound::new();
        entry.put_string("id", "minecraft:not_a_block".to_string());
        unknown.put_compound("BlockState", entry);
        assert_eq!(state_from_nbt(&unknown), Block::SAND.default_state.id);
    }
}
