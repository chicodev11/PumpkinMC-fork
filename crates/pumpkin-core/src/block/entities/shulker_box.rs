use pumpkin_data::data_component_impl::ContainerImpl;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::Taggable;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use std::any::Any;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::BlockDirection;
use pumpkin_util::math::{boundingbox::BoundingBox, vector3::Vector3};
use std::{array::from_fn, sync::Arc};

use crate::block::entities::BlockEntity;
use crate::block::viewer::{ViewerCountListener, ViewerCountTracker, ViewerCountTrackerExt};
use crate::world::World;
use pumpkin_inventory::{Clearable, Inventory, sync_write_items_to_nbt};

pub struct ShulkerBoxBlockEntity {
    pub position: BlockPos,
    pub items: RwLock<[ItemStack; Self::INVENTORY_SIZE]>,
    pub dirty: AtomicBool,
    pub comparator_dirty: AtomicBool,

    // Viewer
    pub viewers: ViewerCountTracker,
    /// Vanilla `AnimationStatus`: closed, opening, opened, closing.
    animation_status: AtomicU8,
    progress: AtomicCell<f32>,
    progress_old: AtomicCell<f32>,
    pub metadata: super::ContainerMetadata,
    /// Vanilla `RandomizableContainerBlockEntity.lootTable`: filled on first open.
    loot_table: std::sync::Mutex<Option<String>>,
    loot_table_seed: i64,
}

impl BlockEntity for ShulkerBoxBlockEntity {
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
        let mut shulker_box = Self::new(position);
        shulker_box.metadata = super::ContainerMetadata::from_nbt(nbt);
        let loot_table = nbt.get_string("LootTable").map(ToString::to_string);
        shulker_box.loot_table_seed = nbt.get_long("LootTableSeed").unwrap_or(0);

        // Vanilla `tryLoadLootTable`: a pending loot table replaces the saved items.
        if loot_table.is_none() {
            pumpkin_inventory::sync_read_items_from_nbt(
                nbt,
                shulker_box
                    .items
                    .get_mut()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
        }
        shulker_box.loot_table = std::sync::Mutex::new(loot_table);

        shulker_box
    }

    fn unpack_loot_table_with(&self, generate: &mut dyn FnMut(String, i64) -> Vec<ItemStack>) {
        if crate::world::loot::unpack_loot_into_slots(
            &self.loot_table,
            self.loot_table_seed,
            &self.items,
            generate,
        )
        .is_some()
        {
            pumpkin_inventory::Inventory::mark_dirty(self);
        }
    }

    fn loot_table_key(&self) -> Option<String> {
        self.loot_table
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn has_loot_table(&self) -> bool {
        self.loot_table
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        let loot_table = self
            .loot_table
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(key) = loot_table {
            nbt.put_string("LootTable", key);
            if self.loot_table_seed != 0 {
                nbt.put_long("LootTableSeed", self.loot_table_seed);
            }
        } else {
            self.write_inventory_nbt(nbt, true);
        }
        self.metadata.write_nbt(nbt);
    }

    fn tick(&self, world: &Arc<World>) {
        self.update_animation(world);
        self.viewers
            .update_viewer_count::<Self>(self, world, &self.position);
    }

    fn refresh_viewers(&self, world: &Arc<World>, user: Option<&dyn crate::entity::EntityBase>) {
        crate::block::viewer::ViewerCountTrackerExt::update_viewer_count_by::<Self>(
            &self.viewers,
            self,
            world,
            &self.position,
            user,
        );
    }

    fn on_block_replaced(self: Arc<Self>, _world: &Arc<World>, _position: &BlockPos) {
        // Shulker boxes retain items when broken
    }

    fn get_inventory(self: Arc<Self>) -> Option<Arc<dyn Inventory>> {
        Some(self)
    }

    fn collect_item_components(&self, stack: &mut ItemStack) {
        self.metadata.collect_item_components(stack);
        let items = self
            .items
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let contents: Vec<(u8, ItemStack)> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.is_empty())
            .map(|(slot, item)| (slot as u8, item.clone()))
            .collect();

        if !contents.is_empty() {
            stack.set_data_component(ContainerImpl { items: contents });
        }
    }

    fn apply_item_components(&self, stack: &ItemStack) {
        self.metadata.apply_item_components(stack);
        if let Some(container) = stack.get_data_component::<ContainerImpl>() {
            let mut items = self
                .items
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            for (slot, item) in &container.items {
                if let Some(target) = items.get_mut(*slot as usize) {
                    *target = item.clone();
                }
            }
        }
        self.mark_dirty();
    }

    fn get_lock(&self) -> Option<pumpkin_data::data_component_impl::LockImpl> {
        self.metadata.get_lock()
    }

    fn get_custom_name(&self) -> Option<pumpkin_util::text::TextComponent> {
        self.metadata.get_custom_name()
    }

    fn drops_for_creative_player(&self) -> bool {
        !self.is_empty()
    }

    fn is_comparator_dirty(&self) -> bool {
        self.comparator_dirty.load(Ordering::Relaxed)
    }

    fn clear_comparator_dirty(&self) {
        self.comparator_dirty.store(false, Ordering::Relaxed);
    }

    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        if let Ok(items) = self.items.try_read() {
            sync_write_items_to_nbt(items.as_slice(), &mut nbt);
        }
        Some(nbt)
    }

    fn bedrock_block_actor_data(
        &self,
        state_id: pumpkin_data::BlockStateId,
    ) -> Option<NbtCompound> {
        let facing =
            match pumpkin_data::block_properties::ShulkerBoxProperties::from_state_id(state_id)
                .facing
            {
                pumpkin_data::block_properties::Facing::Down => 0,
                pumpkin_data::block_properties::Facing::Up => 1,
                pumpkin_data::block_properties::Facing::North => 2,
                pumpkin_data::block_properties::Facing::South => 3,
                pumpkin_data::block_properties::Facing::West => 4,
                pumpkin_data::block_properties::Facing::East => 5,
            };
        Some(build_shulker_box_actor(self.position, facing, None))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ViewerCountListener for ShulkerBoxBlockEntity {
    fn on_container_open(&self, world: &Arc<World>, position: &BlockPos) {
        Self::play_sound(world, position, 1);
    }

    fn on_container_close(&self, world: &Arc<World>, position: &BlockPos) {
        Self::play_sound(world, position, 0);
    }

    fn on_viewer_count_update(&self, world: &Arc<World>, position: &BlockPos, _old: u16, new: u16) {
        world.add_synced_block_event(*position, Self::OPEN_ANIMATION_EVENT_TYPE, new as u8);
    }
}

impl ShulkerBoxBlockEntity {
    pub const INVENTORY_SIZE: usize = 27;
    pub const OPEN_ANIMATION_EVENT_TYPE: u8 = 1;
    pub const ID: &'static str = "minecraft:shulker_box";

    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            items: RwLock::new(from_fn(|_| ItemStack::EMPTY.clone())),
            dirty: AtomicBool::new(false),
            comparator_dirty: AtomicBool::new(false),
            viewers: ViewerCountTracker::new(),
            metadata: super::ContainerMetadata::default(),
            loot_table: std::sync::Mutex::new(None),
            loot_table_seed: 0,
            animation_status: AtomicU8::new(Self::CLOSED),
            progress: AtomicCell::new(0.0),
            progress_old: AtomicCell::new(0.0),
        }
    }

    const CLOSED: u8 = 0;
    const OPENING: u8 = 1;
    const OPENED: u8 = 2;
    const CLOSING: u8 = 3;

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.animation_status.load(Ordering::Relaxed) == Self::CLOSED
    }

    /// Vanilla `triggerEvent`: the open count from the block event starts the lid moving.
    pub fn trigger_event(&self, r#type: u8, data: u8) -> bool {
        if r#type != Self::OPEN_ANIMATION_EVENT_TYPE {
            return false;
        }
        if data == 0 {
            self.animation_status
                .store(Self::CLOSING, Ordering::Relaxed);
        }
        if data == 1 {
            self.animation_status
                .store(Self::OPENING, Ordering::Relaxed);
        }
        true
    }

    fn facing(world: &World, position: &BlockPos) -> BlockDirection {
        use pumpkin_data::FacingExt;
        let state = world.get_block_state(position);
        pumpkin_data::block_properties::EndRodLikeProperties::from_state_id(state.id)
            .facing
            .to_block_direction()
    }

    /// Vanilla `getBoundingBox`: the box with its lid raised, block-local.
    #[must_use]
    pub fn bounding_box(&self, facing: BlockDirection) -> BoundingBox {
        progress_delta_aabb(
            1.0,
            facing,
            -1.0,
            0.5 * self.progress.load(),
            Vector3::new(0.5, 0.0, 0.5),
        )
    }

    /// Vanilla `updateAnimation`.
    fn update_animation(&self, world: &Arc<World>) {
        let progress = self.progress.load();
        self.progress_old.store(progress);
        match self.animation_status.load(Ordering::Relaxed) {
            Self::CLOSED => self.progress.store(0.0),
            Self::OPENING => {
                let progress = progress + 0.1;
                self.progress.store(progress);
                if self.progress_old.load() == 0.0 {
                    Self::do_neighbor_updates(world, &self.position);
                }
                if progress >= 1.0 {
                    self.animation_status.store(Self::OPENED, Ordering::Relaxed);
                    self.progress.store(1.0);
                    Self::do_neighbor_updates(world, &self.position);
                }
                self.move_collided_entities(world);
            }
            Self::CLOSING => {
                let progress = progress - 0.1;
                self.progress.store(progress);
                if self.progress_old.load() == 1.0 {
                    Self::do_neighbor_updates(world, &self.position);
                }
                if progress <= 0.0 {
                    self.animation_status.store(Self::CLOSED, Ordering::Relaxed);
                    self.progress.store(0.0);
                    Self::do_neighbor_updates(world, &self.position);
                }
            }
            _ => self.progress.store(1.0),
        }
    }

    /// Vanilla `doNeighborUpdates`: neighbours see the box's new shape.
    fn do_neighbor_updates(world: &Arc<World>, position: &BlockPos) {
        world.update_neighbors(position, None);
    }

    /// Vanilla `moveCollidedEntities`: the rising lid lifts what stands on it.
    fn move_collided_entities(&self, world: &Arc<World>) {
        if !world
            .get_block(&self.position)
            .has_tag(&pumpkin_data::tag::Block::MINECRAFT_SHULKER_BOXES)
        {
            return;
        }
        let facing = Self::facing(world, &self.position);
        let area = progress_delta_aabb(
            1.0,
            facing,
            self.progress_old.load(),
            self.progress.load(),
            Vector3::new(
                f64::from(self.position.0.x) + 0.5,
                f64::from(self.position.0.y),
                f64::from(self.position.0.z) + 0.5,
            ),
        );
        let entities = world.get_entities_at_box(&area);
        let step = facing.to_offset();
        for entity in entities {
            if crate::block::entities::piston::PistonBlockEntity::ignores_pistons(entity.as_ref()) {
                continue;
            }
            let movement = Vector3::new(
                (area.max.x - area.min.x + 0.01) * f64::from(step.x),
                (area.max.y - area.min.y + 0.01) * f64::from(step.y),
                (area.max.z - area.min.z + 0.01) * f64::from(step.z),
            );
            entity
                .get_entity()
                .move_pushed(entity.as_ref(), movement, false);
        }
    }

    pub fn update_viewers(&self, world: &Arc<World>) {
        let viewer_count = self.viewers.current.load(Ordering::Relaxed);
        Self::play_sound(world, &self.position, i32::from(viewer_count));
    }

    fn play_sound(world: &World, position: &BlockPos, viewer_count: i32) {
        let sound = if viewer_count > 0 {
            Sound::BlockShulkerBoxOpen
        } else {
            Sound::BlockShulkerBoxClose
        };

        world.play_sound(sound, SoundCategory::Blocks, &position.to_f64());
    }
}

const SHULKER_SLOTS: [usize; ShulkerBoxBlockEntity::INVENTORY_SIZE] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26,
];

impl Inventory for ShulkerBoxBlockEntity {
    /// Vanilla `ShulkerBoxBlockEntity.canPlaceItemThroughFace`: no shulker boxes inside.
    fn can_place_item_through_face(
        &self,
        _slot: usize,
        stack: &ItemStack,
        _side: Option<pumpkin_data::BlockDirection>,
    ) -> bool {
        use pumpkin_data::tag::Taggable;
        !stack
            .item
            .has_tag(&pumpkin_data::tag::Item::MINECRAFT_SHULKER_BOXES)
    }

    fn size(&self) -> usize {
        Self::INVENTORY_SIZE
    }

    fn is_empty(&self) -> bool {
        let items = self
            .items
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        items.iter().all(ItemStack::is_empty)
    }

    fn get_stack(&self, slot: usize) -> ItemStack {
        let items = self
            .items
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        items[slot].clone()
    }

    fn remove_stack(&self, slot: usize) -> ItemStack {
        let mut items = self
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let removed = std::mem::replace(&mut items[slot], ItemStack::EMPTY.clone());
        self.mark_dirty();
        removed
    }

    fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
        let mut items = self
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let res = if !items[slot].is_empty() && amount > 0 {
            items[slot].split(amount)
        } else {
            ItemStack::EMPTY.clone()
        };
        self.mark_dirty();
        res
    }

    fn set_stack(&self, slot: usize, stack: ItemStack) {
        let mut items = self
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        items[slot] = stack;
        self.mark_dirty();
    }

    fn update_stack(&self, slot: usize, update: &mut dyn FnMut(&mut ItemStack) -> bool) -> bool {
        pumpkin_inventory::update_locked_stack(&self.items, slot, update)
    }

    fn is_valid_slot_for(&self, _slot: usize, stack: &ItemStack) -> bool {
        !stack
            .item
            .has_tag(&pumpkin_data::tag::Item::MINECRAFT_SHULKER_BOXES)
    }

    fn is_sided(&self) -> bool {
        true
    }

    fn get_slots_for_face(&self, _direction: pumpkin_data::BlockDirection) -> &'static [usize] {
        &SHULKER_SLOTS
    }

    fn on_open(&self) {
        self.viewers.open_container();
    }

    fn on_close(&self) {
        self.viewers.close_container();
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        self.comparator_dirty.store(true, Ordering::Relaxed);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clearable for ShulkerBoxBlockEntity {
    fn clear(&self) {
        let mut items = self
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        items.fill_with(|| ItemStack::EMPTY.clone());
        self.mark_dirty();
    }
}

/// Builds the Bedrock block-actor NBT for shulker boxes.
#[must_use]
pub fn build_shulker_box_actor(
    position: BlockPos,
    facing: u8,
    custom_name: Option<&str>,
) -> NbtCompound {
    super::bedrock::BedrockBlockActorBuilder::new("ShulkerBox", position)
        .movable(false)
        .custom_name(custom_name)
        .put_byte("facing", facing as i8)
        .finish()
}

/// Vanilla `Shulker.getProgressDeltaAabb`: the space the lid sweeps between two progress values.
#[must_use]
pub fn progress_delta_aabb(
    size: f32,
    facing: BlockDirection,
    from: f32,
    to: f32,
    pos: Vector3<f64>,
) -> BoundingBox {
    let size = f64::from(size);
    let half = size * 0.5;
    let base = BoundingBox::new(
        Vector3::new(-half, 0.0, -half),
        Vector3::new(half, size, half),
    );
    let max = f64::from(from.max(to));
    let min = f64::from(from.min(to));
    let step = facing.to_offset();
    let (sx, sy, sz) = (f64::from(step.x), f64::from(step.y), f64::from(step.z));
    let expanded = base.expand_towards(sx * max * size, sy * max * size, sz * max * size);
    contract(
        expanded,
        -sx * (1.0 + min) * size,
        -sy * (1.0 + min) * size,
        -sz * (1.0 + min) * size,
    )
    .shift(pos)
}

/// Vanilla `AABB.contract`.
fn contract(mut aabb: BoundingBox, x: f64, y: f64, z: f64) -> BoundingBox {
    if x < 0.0 {
        aabb.min.x -= x;
    } else if x > 0.0 {
        aabb.max.x -= x;
    }
    if y < 0.0 {
        aabb.min.y -= y;
    } else if y > 0.0 {
        aabb.max.y -= y;
    }
    if z < 0.0 {
        aabb.min.z -= z;
    } else if z > 0.0 {
        aabb.max.z -= z;
    }
    aabb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shulker_box_actor_builder() {
        let pos = BlockPos::new(4, 64, 4);
        let nbt = build_shulker_box_actor(pos, 1, Some("Box"));
        assert_eq!(nbt.get_string("id"), Some("ShulkerBox"));
        assert_eq!(nbt.get_byte("facing"), Some(1));
        assert_eq!(nbt.get_bool("isMovable"), Some(false));
        assert_eq!(nbt.get_string("CustomName"), Some("Box"));
    }
}
