//! Custom (plugin) block entities, and block entities with unknown ids.
//!
//! [`CustomBlockEntity`] stores a type id and arbitrary NBT. Plugins register types with
//! [`register_type`]; a custom block can name one so it is created with the block. A block
//! entity read from disk whose id is neither vanilla nor registered (its plugin is gone) loads as
//! a `CustomBlockEntity` too, so its data is written back unchanged instead of being dropped.
//!
//! Clients never see custom block entities unless the type names a vanilla block entity type
//! to show (`client_type`), with the NBT the plugin provides (`client_nbt`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::chunk::io::Dirtiable;

use super::BlockEntity;
use crate::world::World;

/// Callback bits a custom block entity type can register.
pub mod callback {
    /// Every tick while the chunk is ticking.
    pub const TICK: u32 = 1 << 0;
    /// Created from disk or placed, once it is in a world.
    pub const LOAD: u32 = 1 << 1;
    /// Its chunk unloads.
    pub const UNLOAD: u32 = 1 << 2;
    /// Removed from the world (block broken or replaced).
    pub const REMOVE: u32 = 1 << 3;
}

/// One callback into a block entity type's handler.
pub enum BlockEntityCall {
    Tick {
        world: Arc<World>,
        position: BlockPos,
    },
    Load {
        world: Arc<World>,
        position: BlockPos,
    },
    Unload {
        world: Arc<World>,
        position: BlockPos,
    },
    Remove {
        world: Arc<World>,
        position: BlockPos,
    },
}

/// Answers block entity callbacks (implemented by the plugin host).
pub trait CustomBlockEntityHandler: Send + Sync {
    fn call(&self, ty: &'static CustomBlockEntityType, call: BlockEntityCall);
}

/// A registered custom block entity type.
pub struct CustomBlockEntityType {
    /// `namespace:path`.
    pub id: &'static str,
    /// Vanilla block entity type clients are shown (`sign`, `skull`, ...), if any.
    pub client_type: Option<&'static str>,
    pub callbacks: u32,
    pub handler: Option<Arc<dyn CustomBlockEntityHandler>>,
    /// The owning plugin.
    pub owner: Option<String>,
}

/// Why a type registration was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterTypeError {
    Frozen,
    InvalidId(String),
    Duplicate(String),
    UnknownClientType(String),
}

impl std::fmt::Display for RegisterTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frozen => write!(
                f,
                "custom block entity types can only be registered while plugins load"
            ),
            Self::InvalidId(id) => write!(f, "invalid custom block entity id `{id}`"),
            Self::Duplicate(id) => write!(f, "block entity type `{id}` already exists"),
            Self::UnknownClientType(id) => write!(f, "unknown vanilla block entity type `{id}`"),
        }
    }
}

static REGISTERING: Mutex<Option<HashMap<&'static str, &'static CustomBlockEntityType>>> =
    Mutex::new(None);
static FROZEN: OnceLock<HashMap<&'static str, &'static CustomBlockEntityType>> = OnceLock::new();
static UNKNOWN_IDS: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);

fn is_vanilla_type(id: &str) -> bool {
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    pumpkin_data::block_properties::BLOCK_ENTITY_TYPES.contains(&name)
}

/// Registers a custom block entity type. Only while plugins load.
pub fn register_type(
    ty: CustomBlockEntityType,
) -> Result<&'static CustomBlockEntityType, RegisterTypeError> {
    let mut registering = REGISTERING.lock().unwrap_or_else(PoisonError::into_inner);
    if pumpkin_data::custom_block::is_frozen() || FROZEN.get().is_some() {
        return Err(RegisterTypeError::Frozen);
    }
    let valid = ty.id.split_once(':').is_some_and(|(namespace, path)| {
        !namespace.is_empty() && namespace != "minecraft" && !path.is_empty()
    });
    if !valid {
        return Err(RegisterTypeError::InvalidId(ty.id.to_string()));
    }
    if let Some(client) = ty.client_type
        && !is_vanilla_type(client)
    {
        return Err(RegisterTypeError::UnknownClientType(client.to_string()));
    }
    let map = registering.get_or_insert_with(HashMap::new);
    if map.contains_key(ty.id) {
        return Err(RegisterTypeError::Duplicate(ty.id.to_string()));
    }
    let ty: &'static CustomBlockEntityType = Box::leak(Box::new(ty));
    map.insert(ty.id, ty);
    Ok(ty)
}

/// Fixes the registered types (called with the custom block registry's freeze).
pub fn freeze() {
    let map = REGISTERING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .unwrap_or_default();
    let _ = FROZEN.set(map);
}

/// A registered type by id.
#[must_use]
pub fn get_type(id: &str) -> Option<&'static CustomBlockEntityType> {
    if let Some(map) = FROZEN.get() {
        return map.get(id).copied();
    }
    REGISTERING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()?
        .get(id)
        .copied()
}

/// Interns an unknown id so it can be kept as `&'static str`.
fn intern(id: &str) -> &'static str {
    if let Some(ty) = get_type(id) {
        return ty.id;
    }
    let mut ids = UNKNOWN_IDS.lock().unwrap_or_else(PoisonError::into_inner);
    let ids = ids.get_or_insert_with(HashSet::new);
    if let Some(existing) = ids.get(id) {
        return existing;
    }
    let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
    ids.insert(leaked);
    leaked
}

/// The vanilla block entity type index a client is sent for stored block entity NBT with `id`:
/// vanilla ids map to themselves, custom types to their client type, anything else to `None`
/// (not sent).
#[must_use]
pub fn client_type_index(id: &str) -> Option<usize> {
    let lookup = |name: &str| {
        let name = name.split(':').next_back().unwrap_or(name);
        pumpkin_data::block_properties::BLOCK_ENTITY_TYPES
            .iter()
            .position(|candidate| *candidate == name)
    };
    if id
        .split_once(':')
        .is_none_or(|(namespace, _)| namespace == "minecraft")
    {
        return lookup(id);
    }
    lookup(get_type(id)?.client_type?)
}

/// A custom block entity, or one with an unknown id: a type id plus arbitrary NBT.
pub struct CustomBlockEntity {
    id: &'static str,
    position: BlockPos,
    data: Mutex<NbtCompound>,
    dirty: AtomicBool,
    world: Mutex<Weak<World>>,
    loaded: AtomicBool,
}

impl CustomBlockEntity {
    #[must_use]
    pub fn new(id: &str, position: BlockPos) -> Self {
        Self {
            id: intern(id),
            position,
            data: Mutex::new(NbtCompound::new()),
            dirty: AtomicBool::new(false),
            world: Mutex::new(Weak::new()),
            loaded: AtomicBool::new(false),
        }
    }

    /// The registered type, `None` for an unknown (preserved) id.
    #[must_use]
    pub fn get_type(&self) -> Option<&'static CustomBlockEntityType> {
        get_type(self.id)
    }

    /// A copy of the stored data.
    #[must_use]
    pub fn data(&self) -> NbtCompound {
        self.data
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replaces the stored data and marks the chunk for saving.
    pub fn set_data(&self, data: NbtCompound) {
        *self.data.lock().unwrap_or_else(PoisonError::into_inner) = data;
        self.dirty.store(true, Ordering::Relaxed);
        if let Some(world) = self.world() {
            world
                .level
                .read_chunk_sync(&self.position.chunk_position(), |chunk| {
                    chunk.mark_dirty(true);
                });
        }
    }

    fn world(&self) -> Option<Arc<World>> {
        self.world
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .upgrade()
    }

    fn call(&self, bit: u32, make: impl FnOnce(Arc<World>, BlockPos) -> BlockEntityCall) {
        let Some(ty) = self.get_type() else {
            return;
        };
        if ty.callbacks & bit == 0 {
            return;
        }
        let (Some(handler), Some(world)) = (ty.handler.as_ref(), self.world()) else {
            return;
        };
        handler.call(ty, make(world, self.position));
    }

    /// Called when its chunk unloads.
    pub fn unload(&self) {
        self.call(callback::UNLOAD, |world, position| {
            BlockEntityCall::Unload { world, position }
        });
    }
}

const RESERVED: [&str; 4] = ["id", "x", "y", "z"];

impl BlockEntity for CustomBlockEntity {
    fn write_nbt(&self, nbt: &mut NbtCompound) {
        let data = self.data.lock().unwrap_or_else(PoisonError::into_inner);
        for (key, value) in &data.child_tags {
            if !RESERVED.contains(&&**key) {
                nbt.put(key, value.clone());
            }
        }
    }

    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self {
        let entity = Self::new(nbt.get_string("id").unwrap_or("pumpkin:unknown"), position);
        {
            let mut data = entity.data.lock().unwrap_or_else(PoisonError::into_inner);
            for (key, value) in &nbt.child_tags {
                if !RESERVED.contains(&&**key) {
                    data.put(key, value.clone());
                }
            }
        }
        entity
    }

    fn tick(&self, world: &Arc<World>) {
        let Some(ty) = self.get_type() else {
            return;
        };
        if ty.callbacks & callback::TICK == 0 {
            return;
        }
        if let Some(handler) = ty.handler.as_ref() {
            handler.call(
                ty,
                BlockEntityCall::Tick {
                    world: world.clone(),
                    position: self.position,
                },
            );
        }
    }

    fn resource_location(&self) -> &'static str {
        self.id
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn get_id(&self) -> u32 {
        client_type_index(self.id).unwrap_or(0) as u32
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let ty = self.get_type()?;
        ty.client_type?;
        // The plugin chooses what the client sees: the `client_nbt` compound of its data.
        let data = self.data.lock().unwrap_or_else(PoisonError::into_inner);
        Some(data.get_compound("client_nbt").cloned().unwrap_or_default())
    }

    fn set_world(&self, world: Weak<World>) {
        *self.world.lock().unwrap_or_else(PoisonError::into_inner) = world;
        if !self.loaded.swap(true, Ordering::Relaxed) {
            self.call(callback::LOAD, |world, position| BlockEntityCall::Load {
                world,
                position,
            });
        }
    }

    fn on_block_replaced(self: Arc<Self>, _world: &Arc<World>, _position: &BlockPos) {
        self.call(callback::REMOVE, |world, position| {
            BlockEntityCall::Remove { world, position }
        });
    }

    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_nbt::tag::NbtTag;

    #[test]
    fn unknown_ids_keep_their_data() {
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", "gone_plugin:machine".to_string());
        nbt.put_int("x", 1);
        nbt.put_int("y", 2);
        nbt.put_int("z", 3);
        nbt.put_int("energy", 42);
        let mut inner = NbtCompound::new();
        inner.put_string("owner", "steve".to_string());
        nbt.put_compound("state", inner);

        let entity = super::super::block_entity_from_nbt(&nbt).expect("kept, not dropped");
        assert_eq!(entity.resource_location(), "gone_plugin:machine");
        let mut written = NbtCompound::new();
        entity.write_internal(&mut written);
        assert_eq!(written.get_string("id"), Some("gone_plugin:machine"));
        assert_eq!(written.get_int("x"), Some(1));
        assert_eq!(written.get_int("energy"), Some(42));
        assert!(matches!(written.get("state"), Some(NbtTag::Compound(_))));
        // Clients are never sent unknown types.
        assert!(entity.chunk_data_nbt().is_none());
        assert_eq!(client_type_index("gone_plugin:machine"), None);
        assert!(client_type_index("minecraft:chest").is_some());
        assert!(client_type_index("chest").is_some());
    }
}
