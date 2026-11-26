use crate::block::entities::BlockEntity;
use crate::world::World;
use pumpkin_data::block_properties::{FacingHopper, HopperLikeProperties};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{BlockDirection, BlockId, BlockStateId};
use pumpkin_inventory::{Clearable, Inventory, sync_write_items_to_nbt};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use std::any::Any;
use std::array::from_fn;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64};

pub struct HopperBlockEntity {
    pub position: BlockPos,
    pub items: RwLock<[ItemStack; Self::INVENTORY_SIZE]>,
    pub dirty: AtomicBool,
    pub comparator_dirty: AtomicBool,
    pub facing: FacingHopper,
    pub cooldown_time: AtomicI32,
    pub ticked_game_time: AtomicI64,
    pub metadata: super::ContainerMetadata,
}

#[must_use]
pub fn to_offset(facing: &FacingHopper) -> Vector3<i32> {
    match facing {
        FacingHopper::Down => (0, -1, 0),
        FacingHopper::North => (0, 0, -1),
        FacingHopper::South => (0, 0, 1),
        FacingHopper::West => (-1, 0, 0),
        FacingHopper::East => (1, 0, 0),
    }
    .into()
}

/// Properties of one state snapshot, `None` for any other block. `from_state_id` parses whatever
/// it is handed, so only the block id can reject a replacement state.
fn hopper_properties(block: BlockId, state_id: BlockStateId) -> Option<HopperLikeProperties> {
    (block == BlockId::HOPPER).then(|| HopperLikeProperties::from_state_id(state_id))
}

/// One item taken out of a slot, with what the slot held before and after the removal.
///
/// A compare-and-swap: the offer runs without the source lock (holding it across
/// [`HopperBlockEntity::add_one_item`] deadlocks two hoppers facing each other), so the rollback
/// has to compare against `remainder` to tell an untouched slot from someone else's write.
struct Extraction {
    one_item: ItemStack,
    snapshot: ItemStack,
    remainder: ItemStack,
}

impl BlockEntity for HopperBlockEntity {
    fn write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put(
            "TransferCooldown",
            NbtTag::Int(self.cooldown_time.load(Ordering::Relaxed)),
        );
        self.write_inventory_nbt(nbt, true);
        self.metadata.write_nbt(nbt);
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let mut hopper = Self {
            position,
            items: RwLock::new(from_fn(|_| ItemStack::EMPTY.clone())),
            dirty: AtomicBool::new(false),
            comparator_dirty: AtomicBool::new(false),
            facing: FacingHopper::Down,
            cooldown_time: AtomicI32::from(nbt.get_int("TransferCooldown").unwrap_or(-1)),
            ticked_game_time: AtomicI64::new(0),
            metadata: super::ContainerMetadata::from_nbt(nbt),
        };

        pumpkin_inventory::sync_read_items_from_nbt(
            nbt,
            hopper
                .items
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );

        hopper
    }

    fn tick(&self, world: &Arc<World>) {
        self.ticked_game_time
            .store(world.get_world_age(), Ordering::Relaxed);
        // The block entity outlives its block by a tick when another Rayon worker replaces it,
        // so guard like `trial_spawner.rs::tick` does. One snapshot for id and state: a second
        // read could already be the replacement, and the pair would not belong together.
        let (block, state) = world.get_block_and_state(&self.position);
        let Some(properties) = hopper_properties(block.id, state.id) else {
            return;
        };
        // Vanilla `pushItemsTick`.
        self.cooldown_time.fetch_sub(1, Ordering::Relaxed);
        if !self.is_on_cooldown() {
            self.cooldown_time.store(0, Ordering::Relaxed);
            if let Some(entity) = world.get_block_entity(&self.position)
                && let Some(hopper) = entity.as_any().downcast_ref::<Self>()
            {
                hopper.try_move_items(properties, world, |hopper| {
                    suck_in_items(
                        world,
                        hopper,
                        hopper.position.to_centered_f64(),
                        true,
                        Some(hopper.position),
                    )
                });
            }
        }
    }

    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn get_inventory(self: Arc<Self>) -> Option<Arc<dyn Inventory>> {
        Some(self)
    }

    fn set_block_state(&mut self, block_state: BlockStateId) {
        // Transfers read the facing from the block state, so a reloaded hopper (whose block
        // entity is created facing down) still pushes the right way.
        self.facing = HopperLikeProperties::from_state_id(block_state).facing;
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
        nbt.put(
            "TransferCooldown",
            NbtTag::Int(self.cooldown_time.load(Ordering::Relaxed)),
        );
        if let Ok(items) = self.items.try_read() {
            sync_write_items_to_nbt(items.as_slice(), &mut nbt);
        }
        Some(nbt)
    }

    fn bedrock_block_actor_data(
        &self,
        _state_id: pumpkin_data::BlockStateId,
    ) -> Option<NbtCompound> {
        Some(crate::block::entities::bedrock::build_container_actor(
            "Hopper",
            self.position,
            None,
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn apply_item_components(&self, stack: &ItemStack) {
        self.metadata.apply_item_components(stack);
        let mut items = self
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        super::apply_container_items_to_slice(stack, &mut *items);
    }

    fn collect_item_components(&self, stack: &mut ItemStack) {
        self.metadata.collect_name_only(stack);
    }

    fn collect_pick_item_components(&self, stack: &mut ItemStack) {
        self.metadata.collect_item_components(stack);
        let items = self
            .items
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        super::collect_container_items_from_slice(&*items, stack);
    }

    fn get_lock(&self) -> Option<pumpkin_data::data_component_impl::LockImpl> {
        self.metadata.get_lock()
    }

    fn get_custom_name(&self) -> Option<pumpkin_util::text::TextComponent> {
        self.metadata.get_custom_name()
    }
}

impl HopperBlockEntity {
    pub const INVENTORY_SIZE: usize = 5;
    pub const ID: &'static str = "minecraft:hopper";

    #[must_use]
    pub fn new(position: BlockPos, facing: FacingHopper) -> Self {
        Self {
            position,
            items: RwLock::new(from_fn(|_| ItemStack::EMPTY.clone())),
            dirty: AtomicBool::new(false),
            comparator_dirty: AtomicBool::new(false),
            facing,
            cooldown_time: AtomicI32::new(-1),
            ticked_game_time: AtomicI64::new(0),
            metadata: super::ContainerMetadata::default(),
        }
    }
    /// Vanilla `HopperBlockEntity.tryMoveItems`; `suck` is `suckInItems` or, for an item
    /// landing in the hopper, `addItem` of that item.
    fn try_move_items(
        &self,
        state: HopperLikeProperties,
        world: &Arc<World>,
        suck: impl FnOnce(&Self) -> bool,
    ) -> bool {
        if self.is_on_cooldown() || !state.enabled {
            return false;
        }
        let mut success = !self.is_empty() && self.eject_items(world, state.facing);
        if !self.inventory_full() {
            success |= suck(self);
        }
        if success {
            self.cooldown_time.store(8, Ordering::Relaxed);
            self.mark_dirty();
        }
        success
    }

    fn is_on_cooldown(&self) -> bool {
        self.cooldown_time.load(Ordering::Relaxed) > 0
    }

    /// Vanilla `isOnCustomCooldown`: a cooldown longer than the normal 8 ticks.
    fn is_on_custom_cooldown(&self) -> bool {
        self.cooldown_time.load(Ordering::Relaxed) > 8
    }

    fn inventory_full(&self) -> bool {
        let items = self
            .items
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        items
            .iter()
            .all(|item| !item.is_empty() && item.item_count == item.get_max_stack_size())
    }

    /// Vanilla `HopperBlockEntity.entityInside`: an item touching the hopper's intake goes in
    /// at once.
    pub fn entity_inside(&self, world: &Arc<World>, entity: &dyn crate::entity::EntityBase) {
        let Some(item) = entity.get_item_entity() else {
            return;
        };
        let is_empty = item
            .get_item_stack()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty();
        if is_empty {
            return;
        }
        let suck_box = suck_aabb(self.position.to_centered_f64());
        if !entity
            .get_entity()
            .bounding_box
            .load()
            .intersects(&suck_box)
        {
            return;
        }
        let (block, state) = world.get_block_and_state(&self.position);
        let Some(properties) = hopper_properties(block.id, state.id) else {
            return;
        };
        self.try_move_items(properties, world, |hopper| add_item_entity(hopper, item));
    }

    /// Splits one item off `slot`. One lock for read and write, so the snapshot is the state the
    /// removal really happened on and not an older one. `None` when the slot is empty by then.
    fn take_one(&self, slot: usize) -> Option<Extraction> {
        let mut items = self
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if items[slot].is_empty() {
            return None;
        }
        let snapshot = items[slot].clone();
        let one_item = items[slot].split(1);
        let remainder = items[slot].clone();
        self.mark_dirty();
        Some(Extraction {
            one_item,
            snapshot,
            remainder,
        })
    }

    /// Undoes [`Self::take_one`] after a failed offer, handing the item back when the slot has no
    /// room for it any more.
    ///
    /// The snapshot only fits a slot nobody wrote to, so it is restored on a match and dropped on
    /// a mismatch, writing it anyway would undo the other write, in either direction.
    fn put_back(&self, slot: usize, extraction: Extraction) -> Option<ItemStack> {
        let mut items = self
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.mark_dirty();
        let current = &mut items[slot];
        if current.are_equal(&extraction.remainder) {
            *current = extraction.snapshot;
            return None;
        }
        if current.is_empty() {
            *current = extraction.one_item;
            return None;
        }
        if current.are_items_and_components_equal(&extraction.one_item)
            && current.item_count < current.get_max_stack_size()
        {
            current.item_count += 1;
            return None;
        }
        Some(extraction.one_item)
    }

    /// Vanilla `HopperBlockEntity.ejectItems`, into the container the hopper faces.
    fn eject_items(&self, world: &Arc<World>, facing: FacingHopper) -> bool {
        let target_pos = self.position.offset(to_offset(&facing));
        let Some(container) = get_container_at(world, &target_pos, target_pos.to_centered_f64())
        else {
            return false;
        };
        let face = facing_to_direction(facing).opposite();
        if is_full_container(container.as_ref(), face) {
            return false;
        }
        for slot in 0..Self::INVENTORY_SIZE {
            let item = self.get_stack(slot);
            if item.is_empty() {
                continue;
            }
            let mut move_event = crate::plugin::api::events::inventory::inventory_move_item::InventoryMoveItemEvent::new(
                self.position,
                target_pos,
                item.item.registry_key.to_string(),
                1,
            );
            if let Some(server) = world.server.upgrade() {
                server
                    .plugin_manager
                    .fire_blocking(&server, &mut move_event);
            }
            if move_event.cancelled {
                continue;
            }
            // Take the item out before offering it and restore it on failure, like vanilla's
            // `removeItem` + `setCount`, without holding the lock across the offer.
            let Some(extraction) = self.take_one(slot) else {
                continue;
            };
            let remainder = add_item(
                Some(self),
                container.as_ref(),
                extraction.one_item.clone(),
                Some(face),
            );
            if remainder.is_empty() {
                container.mark_dirty();
                return true;
            }
            if let Some(leftover) = self.put_back(slot, extraction) {
                // Slot is someone else's now and full -> dropping beats overwriting or voiding.
                let pos = self.position.to_centered_f64();
                world.scatter_stack(pos.x, pos.y, pos.z, leftover);
            }
        }
        false
    }

    /// Offers one item to `to` through no particular face; `true` when it went in.
    pub fn add_one_item(from: &dyn Inventory, to: &dyn Inventory, item: &ItemStack) -> bool {
        add_item(Some(from), to, item.clone(), None).is_empty()
    }
}

const fn facing_to_direction(facing: FacingHopper) -> BlockDirection {
    match facing {
        FacingHopper::Down => BlockDirection::Down,
        FacingHopper::North => BlockDirection::North,
        FacingHopper::South => BlockDirection::South,
        FacingHopper::West => BlockDirection::West,
        FacingHopper::East => BlockDirection::East,
    }
}

/// Vanilla `Hopper.SUCK_AABB` moved to a hopper whose `getLevelX/Y/Z` is `level`: the block
/// column from 11/16 up to the top of the block above.
fn suck_aabb(level: Vector3<f64>) -> BoundingBox {
    BoundingBox::new(
        Vector3::new(level.x - 0.5, level.y - 0.5 + 0.6875, level.z - 0.5),
        Vector3::new(level.x + 0.5, level.y + 1.5, level.z + 0.5),
    )
}

fn is_worldly(container: &dyn Inventory) -> bool {
    container.is_sided()
}

/// Vanilla `HopperBlockEntity.getSlots`.
fn get_slots(container: &dyn Inventory, face: BlockDirection) -> Vec<usize> {
    if container.is_sided() {
        container.get_slots_for_face(face).to_vec()
    } else {
        (0..container.size()).collect()
    }
}

/// Vanilla `HopperBlockEntity.isFullContainer`.
fn is_full_container(container: &dyn Inventory, face: BlockDirection) -> bool {
    get_slots(container, face).into_iter().all(|slot| {
        let stack = container.get_stack(slot);
        stack.item_count >= stack.get_max_stack_size()
    })
}

/// Vanilla `HopperBlockEntity.suckInItems` for a block hopper or a hopper minecart whose
/// `getLevelX/Y/Z` is `level`: from the container above, else from item entities on top.
pub fn suck_in_items(
    world: &Arc<World>,
    hopper: &dyn Inventory,
    level: Vector3<f64>,
    grid_aligned: bool,
    event_position: Option<BlockPos>,
) -> bool {
    let above = BlockPos::floored(level.x, level.y + 1.0, level.z);
    if let Some(position) = event_position {
        let mut search_event = crate::plugin::api::events::inventory::hopper_inventory_search::HopperInventorySearchEvent::new(
            position,
            above,
        );
        if let Some(server) = world.server.upgrade() {
            server
                .plugin_manager
                .fire_blocking(&server, &mut search_event);
        }
        if search_event.cancelled {
            return false;
        }
    }
    if let Some(source) =
        get_container_at(world, &above, Vector3::new(level.x, level.y + 1.0, level.z))
    {
        return get_slots(source.as_ref(), BlockDirection::Down)
            .into_iter()
            .any(|slot| try_take_in_item_from_slot(world, level, hopper, source.as_ref(), slot));
    }
    let (block, state) = world.get_block_and_state(&above);
    let blocked = grid_aligned
        && state.is_full_cube()
        && !block.has_tag(&tag::Block::MINECRAFT_DOES_NOT_BLOCK_HOPPERS);
    if blocked {
        return false;
    }
    for entity in world.get_entities_at_box(&suck_aabb(level)) {
        let Some(item) = entity.get_item_entity() else {
            continue;
        };
        if entity.get_entity().is_removed() {
            continue;
        }
        if let Some(position) = event_position {
            let registry_key = item
                .get_item_stack()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .item
                .registry_key
                .to_string();
            let mut pickup_event =
                crate::plugin::api::events::inventory::inventory_pickup_item::InventoryPickupItemEvent::new(
                    position,
                    item.get_entity().entity_id,
                    registry_key,
                );
            if let Some(server) = world.server.upgrade() {
                server
                    .plugin_manager
                    .fire_blocking(&server, &mut pickup_event);
            }
            if pickup_event.cancelled {
                continue;
            }
        }
        if add_item_entity(hopper, item) {
            return true;
        }
    }
    false
}

/// Vanilla `HopperBlockEntity.tryTakeInItemFromSlot`.
fn try_take_in_item_from_slot(
    world: &Arc<World>,
    level: Vector3<f64>,
    hopper: &dyn Inventory,
    source: &dyn Inventory,
    slot: usize,
) -> bool {
    let stack = source.get_stack(slot);
    if stack.is_empty() || !can_take_item_from_container(hopper, source, &stack, slot) {
        return false;
    }
    let one = source.remove_stack_specific(slot, 1);
    if one.is_empty() {
        return false;
    }
    let remainder = add_item(Some(source), hopper, one.clone(), None);
    if remainder.is_empty() {
        source.mark_dirty();
        return true;
    }
    // Give the item back; vanilla's `setCount` on the same stack.
    let current = source.get_stack(slot);
    if current.is_empty() {
        source.set_stack(slot, one);
    } else if current.are_items_and_components_equal(&one)
        && current.item_count < current.get_max_stack_size()
    {
        let mut current = current;
        current.item_count += 1;
        source.set_stack(slot, current);
    } else {
        // Someone else filled the slot meanwhile: dropping beats voiding the item.
        world.scatter_stack(level.x, level.y + 1.0, level.z, one);
    }
    false
}

/// Vanilla `HopperBlockEntity.addItem(Container, ItemEntity)`.
pub fn add_item_entity(to: &dyn Inventory, item: &crate::entity::item::ItemEntity) -> bool {
    let copy = item
        .get_item_stack()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if copy.is_empty() {
        return false;
    }
    let remainder = add_item(None, to, copy, None);
    let mut stack = item
        .get_item_stack()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if remainder.is_empty() {
        *stack = ItemStack::EMPTY.clone();
        drop(stack);
        item.get_entity().remove();
        true
    } else {
        *stack = remainder;
        false
    }
}

/// Vanilla `HopperBlockEntity.addItem(from, to, stack, face)`: offers `stack` slot by slot
/// and returns what did not fit.
pub fn add_item(
    from: Option<&dyn Inventory>,
    to: &dyn Inventory,
    mut stack: ItemStack,
    face: Option<BlockDirection>,
) -> ItemStack {
    let slots = match face {
        Some(face) if to.is_sided() => to.get_slots_for_face(face).to_vec(),
        _ => (0..to.size()).collect(),
    };
    for slot in slots {
        if stack.is_empty() {
            break;
        }
        stack = try_move_in_item(from, to, stack, slot, face);
    }
    stack
}

/// Vanilla `HopperBlockEntity.canPlaceItemInContainer`.
fn can_place_item_in_container(
    to: &dyn Inventory,
    stack: &ItemStack,
    slot: usize,
    face: Option<BlockDirection>,
) -> bool {
    to.is_valid_slot_for(slot, stack)
        && (!is_worldly(to) || to.can_place_item_through_face(slot, stack, face))
}

/// Vanilla `HopperBlockEntity.canTakeItemFromContainer`, always through the bottom face.
fn can_take_item_from_container(
    target: &dyn Inventory,
    source: &dyn Inventory,
    stack: &ItemStack,
    slot: usize,
) -> bool {
    source.can_transfer_to(target, slot, stack)
        && (!is_worldly(source)
            || source.can_take_item_through_face(slot, stack, BlockDirection::Down))
}

/// Vanilla `HopperBlockEntity.tryMoveInItem`.
fn try_move_in_item(
    from: Option<&dyn Inventory>,
    to: &dyn Inventory,
    mut stack: ItemStack,
    slot: usize,
    face: Option<BlockDirection>,
) -> ItemStack {
    if !can_place_item_in_container(to, &stack, slot, face) {
        return stack;
    }
    let mut item = to.get_stack(slot);
    let was_empty = to.is_empty();
    let success = if item.is_empty() {
        to.set_stack(slot, stack);
        stack = ItemStack::EMPTY.clone();
        true
    } else if item.item_count <= item.get_max_stack_size()
        && item.are_items_and_components_equal(&stack)
    {
        let space = stack.get_max_stack_size().saturating_sub(item.item_count);
        let moved = stack.item_count.min(space);
        stack.item_count -= moved;
        item.item_count += moved;
        to.set_stack(slot, item);
        moved > 0
    } else {
        false
    };
    if success {
        if was_empty
            && let Some(hopper) = to.as_any().downcast_ref::<HopperBlockEntity>()
            && !hopper.is_on_custom_cooldown()
        {
            // A hopper fed by a hopper that already ticked this game tick waits one tick less.
            let offset = from
                .and_then(|from| from.as_any().downcast_ref::<HopperBlockEntity>())
                .is_some_and(|from| {
                    hopper.ticked_game_time.load(Ordering::Relaxed)
                        >= from.ticked_game_time.load(Ordering::Relaxed)
                });
            hopper
                .cooldown_time
                .store(8 - i32::from(offset), Ordering::Relaxed);
        }
        to.mark_dirty();
    }
    stack
}

/// Vanilla `HopperBlockEntity.getContainerAt`: the block's container, else a random container
/// entity (chest/hopper minecart) around `center`.
pub fn get_container_at(
    world: &Arc<World>,
    pos: &BlockPos,
    center: Vector3<f64>,
) -> Option<Arc<dyn Inventory>> {
    get_block_container(world, pos).or_else(|| get_entity_container(world, center))
}

/// Vanilla `HopperBlockEntity.getBlockContainer`.
fn get_block_container(world: &Arc<World>, pos: &BlockPos) -> Option<Arc<dyn Inventory>> {
    let (block, state) = world.get_block_and_state(pos);
    if block == &pumpkin_data::Block::COMPOSTER {
        return Some(
            crate::block::blocks::composter::ComposterBlock::get_container(state.id, world, pos),
        );
    }
    let entity = world.get_block_entity(pos)?;
    // Lecterns and beacons hold items but are not vanilla containers.
    if matches!(
        entity.resource_location(),
        "minecraft:lectern" | "minecraft:beacon"
    ) {
        return None;
    }
    // Randomizable containers fill their loot on first access, hoppers included.
    if entity.has_loot_table() {
        crate::block::blocks::chests::unpack_chest_loot(&entity, world, pos, None);
    }
    let inventory = entity.clone().get_inventory()?;
    if crate::block::blocks::chests::is_chest_block(block) {
        return Some(crate::block::blocks::chests::combined_chest_inventory(
            world, pos, block, state.id, inventory,
        ));
    }
    Some(inventory)
}

/// Vanilla `HopperBlockEntity.getEntityContainer`: a random live container entity within half
/// a block of `center`.
fn get_entity_container(world: &Arc<World>, center: Vector3<f64>) -> Option<Arc<dyn Inventory>> {
    let search = BoundingBox::new(
        Vector3::new(center.x - 0.5, center.y - 0.5, center.z - 0.5),
        Vector3::new(center.x + 0.5, center.y + 0.5, center.z + 0.5),
    );
    let containers: Vec<_> = world
        .get_entities_at_box(&search)
        .into_iter()
        // `EntitySelector.CONTAINER_ENTITY_SELECTOR`: every `Container` entity (minecarts and
        // chest boats), alive. Reading one unpacks its loot table like vanilla `ContainerEntity`.
        .filter(|entity| !entity.get_entity().is_removed() && entity.get_entity().is_alive())
        .filter_map(|entity| entity.get_container_inventory())
        .collect();
    if containers.is_empty() {
        return None;
    }
    let index = rand::random_range(0..containers.len());
    containers.into_iter().nth(index)
}

impl Inventory for HopperBlockEntity {
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

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        self.comparator_dirty.store(true, Ordering::Relaxed);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clearable for HopperBlockEntity {
    fn clear(&self) {
        let mut items = self
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        items.fill_with(|| ItemStack::EMPTY.clone());
        self.mark_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::{Block, item::Item};

    #[test]
    fn hopper_state_yields_its_properties() {
        let properties =
            hopper_properties(Block::HOPPER.id, Block::HOPPER.default_state.id).unwrap();

        assert!(properties.enabled);
        assert_eq!(properties.facing, FacingHopper::Down);
    }

    #[test]
    fn disabled_hopper_state_is_read_as_disabled() {
        let disabled = HopperLikeProperties {
            facing: FacingHopper::North,
            enabled: false,
        }
        .to_state_id(&Block::HOPPER);

        let properties = hopper_properties(Block::HOPPER.id, disabled).unwrap();

        assert!(!properties.enabled);
        assert_eq!(properties.facing, FacingHopper::North);
    }

    /// Every one of these decodes cleanly through `from_state_id` into some `facing`/`enabled`.
    /// Nothing about the state marks it as foreign, so rejection has to come from the id.
    #[test]
    fn replacement_state_yields_no_properties() {
        for replacement in [Block::AIR, Block::CHEST, Block::DROPPER, Block::PISTON] {
            assert!(
                hopper_properties(replacement.id, replacement.default_state.id).is_none(),
                "{} was accepted as a hopper",
                replacement.name
            );
        }
    }

    /// The other direction: a hopper state under a foreign id is still foreign. Pins that the id
    /// decides, not a range check on the state.
    #[test]
    fn hopper_state_id_under_another_block_yields_no_properties() {
        assert!(hopper_properties(Block::CHEST.id, Block::HOPPER.default_state.id).is_none());
    }

    fn hopper_holding(stack: ItemStack) -> HopperBlockEntity {
        let hopper = HopperBlockEntity::new(BlockPos::new(0, 0, 0), FacingHopper::Down);
        hopper.set_stack(0, stack);
        hopper
    }

    #[test]
    fn take_one_splits_a_single_item_off() {
        let hopper = hopper_holding(ItemStack::new(10, &Item::DIAMOND));

        let extraction = hopper.take_one(0).unwrap();

        assert_eq!(extraction.one_item.item_count, 1);
        assert_eq!(extraction.snapshot.item_count, 10);
        assert_eq!(extraction.remainder.item_count, 9);
        assert_eq!(hopper.get_stack(0).item_count, 9);
    }

    #[test]
    fn take_one_on_an_empty_slot_extracts_nothing() {
        let hopper = hopper_holding(ItemStack::EMPTY.clone());

        assert!(hopper.take_one(0).is_none());
    }

    #[test]
    fn untouched_slot_gets_the_snapshot_back() {
        let hopper = hopper_holding(ItemStack::new(10, &Item::DIAMOND));
        let extraction = hopper.take_one(0).unwrap();

        assert!(hopper.put_back(0, extraction).is_none());
        assert_eq!(hopper.get_stack(0).item_count, 10);
    }

    /// The snapshot says 10, the slot says 3 because someone took 7 while the offer was out.
    /// Restoring the snapshot would conjure those 7 back, so only the one item returns.
    #[test]
    fn changed_count_takes_back_one_item_not_the_snapshot() {
        let hopper = hopper_holding(ItemStack::new(10, &Item::DIAMOND));
        let extraction = hopper.take_one(0).unwrap();
        hopper.set_stack(0, ItemStack::new(3, &Item::DIAMOND));

        assert!(hopper.put_back(0, extraction).is_none());
        assert_eq!(hopper.get_stack(0).item_count, 4);
    }

    /// A foreign item cannot absorb the one item and must not be overwritten, so nothing fits and
    /// the item comes back out.
    #[test]
    fn foreign_item_in_the_slot_is_left_alone() {
        let hopper = hopper_holding(ItemStack::new(1, &Item::DIAMOND));
        let extraction = hopper.take_one(0).unwrap();
        hopper.set_stack(0, ItemStack::new(64, &Item::DIRT));

        let leftover = hopper.put_back(0, extraction).unwrap();

        assert_eq!(leftover.get_item().id, Item::DIAMOND.id);
        assert_eq!(leftover.item_count, 1);
        let current = hopper.get_stack(0);
        assert_eq!(current.get_item().id, Item::DIRT.id);
        assert_eq!(current.item_count, 64);
    }

    /// Emptied in the meantime, so there is room and nothing to overwrite.
    #[test]
    fn emptied_slot_takes_the_single_item() {
        let hopper = hopper_holding(ItemStack::new(10, &Item::DIAMOND));
        let extraction = hopper.take_one(0).unwrap();
        hopper.set_stack(0, ItemStack::EMPTY.clone());

        assert!(hopper.put_back(0, extraction).is_none());
        assert_eq!(hopper.get_stack(0).item_count, 1);
    }

    /// Same item but no room left. Incrementing would push it past `get_max_stack_size`.
    #[test]
    fn full_slot_of_the_same_item_hands_the_item_back() {
        let hopper = hopper_holding(ItemStack::new(10, &Item::DIAMOND));
        let extraction = hopper.take_one(0).unwrap();
        let max = ItemStack::new(1, &Item::DIAMOND).get_max_stack_size();
        hopper.set_stack(0, ItemStack::new(max, &Item::DIAMOND));

        let leftover = hopper.put_back(0, extraction).unwrap();

        assert_eq!(leftover.item_count, 1);
        assert_eq!(hopper.get_stack(0).item_count, max);
    }
}
