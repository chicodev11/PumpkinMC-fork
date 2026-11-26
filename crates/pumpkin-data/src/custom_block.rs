//! Runtime (plugin) blocks, appended to the id space right after the vanilla registry.
//!
//! Vanilla states keep the ids `0..BlockStateId::STATE_COUNT`. Custom states get the ids after
//! them, custom blocks the ids after `BlockId::BLOCK_COUNT`. A registration leaks its [`Block`],
//! its contiguous [`BlockState`] slice and the extra data below, stores the pointers in
//! fixed-capacity tables and then publishes the new count. Readers never lock: the generated
//! lookups check `id < vanilla count` first and only then load one pointer from here, so a
//! server without custom blocks pays a single predictable branch.
//!
//! Plugins register while they load; [`freeze`] closes the registry before the worlds load.
//! After that only placeholders are added: a block read from disk whose name is unknown (its
//! plugin is gone) becomes a single-state placeholder block that writes back exactly the name
//! and properties it was read with, so nothing is lost.
//!
//! Every custom state names a vanilla state (`client_state`) that clients are shown instead;
//! [`crate::block_remap`] applies it to every Java and Bedrock packet.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU16, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::vector3::Vector3;

use crate::block_properties::{COLLISION_SHAPES, NoteblockInstrument, SOUND_TYPES};
use crate::block_state::{PistonBehavior, flags};
use crate::blocks::Flammable;
use crate::{Block, BlockId, BlockState, BlockStateId};

/// `BlockState::block_entity_type` of custom states that create a custom block entity.
pub const CUSTOM_BLOCK_ENTITY_TYPE: u16 = u16::MAX - 1;

/// Number of vanilla block states; custom state ids start here.
pub const VANILLA_STATE_COUNT: u16 = BlockStateId::STATE_COUNT;
/// Number of vanilla blocks; custom block ids start here.
pub const VANILLA_BLOCK_COUNT: u16 = BlockId::BLOCK_COUNT;

/// Custom states fit in the `u16` id space after vanilla (`u16::MAX` itself stays unused).
const STATE_CAPACITY: usize = (u16::MAX - VANILLA_STATE_COUNT) as usize;
const BLOCK_CAPACITY: usize = 8192;
const SHAPE_CAPACITY: usize = 16384;

#[expect(clippy::declare_interior_mutable_const)]
const NULL_STATE: AtomicPtr<BlockState> = AtomicPtr::new(std::ptr::null_mut());
#[expect(clippy::declare_interior_mutable_const)]
const NULL_STATE_INFO: AtomicPtr<CustomStateInfo> = AtomicPtr::new(std::ptr::null_mut());
#[expect(clippy::declare_interior_mutable_const)]
const NULL_BLOCK: AtomicPtr<Block> = AtomicPtr::new(std::ptr::null_mut());
#[expect(clippy::declare_interior_mutable_const)]
const NULL_BLOCK_INFO: AtomicPtr<CustomBlockInfo> = AtomicPtr::new(std::ptr::null_mut());
#[expect(clippy::declare_interior_mutable_const)]
const NULL_SHAPE: AtomicPtr<BoundingBox> = AtomicPtr::new(std::ptr::null_mut());

static STATES: [AtomicPtr<BlockState>; STATE_CAPACITY] = [NULL_STATE; STATE_CAPACITY];
static STATE_INFOS: [AtomicPtr<CustomStateInfo>; STATE_CAPACITY] =
    [NULL_STATE_INFO; STATE_CAPACITY];
static BLOCKS: [AtomicPtr<Block>; BLOCK_CAPACITY] = [NULL_BLOCK; BLOCK_CAPACITY];
static BLOCK_INFOS: [AtomicPtr<CustomBlockInfo>; BLOCK_CAPACITY] =
    [NULL_BLOCK_INFO; BLOCK_CAPACITY];
static SHAPES: [AtomicPtr<BoundingBox>; SHAPE_CAPACITY] = [NULL_SHAPE; SHAPE_CAPACITY];

/// Published counts. A slot below the count is always filled.
static STATE_COUNT: AtomicU16 = AtomicU16::new(0);
static BLOCK_COUNT: AtomicU16 = AtomicU16::new(0);

static FROZEN: AtomicBool = AtomicBool::new(false);

/// Writers (registration, placeholders, shapes) take this lock; readers never do.
static WRITER: Mutex<Writer> = Mutex::new(Writer {
    names: None,
    placeholders: None,
    shapes: None,
    shape_count: 0,
});

/// Name map for real custom blocks, built once at [`freeze`] so lookups after it are lock free.
static FROZEN_NAMES: OnceLock<HashMap<&'static str, BlockId>> = OnceLock::new();

struct Writer {
    names: Option<HashMap<&'static str, BlockId>>,
    placeholders: Option<HashMap<String, BlockStateId>>,
    shapes: Option<HashMap<[u64; 6], u16>>,
    shape_count: u16,
}

fn writer() -> std::sync::MutexGuard<'static, Writer> {
    WRITER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One string property of a custom block: its name and its values in state order.
#[derive(Debug)]
pub struct CustomProperty {
    pub name: &'static str,
    pub values: Box<[&'static str]>,
}

/// Sounds named by string id (resource pack sounds) instead of a vanilla sound group.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomSoundGroup {
    pub volume: f32,
    pub pitch: f32,
    pub break_sound: String,
    pub step_sound: String,
    pub place_sound: String,
    pub hit_sound: String,
    pub fall_sound: String,
}

/// Extra data of a custom block, next to its [`Block`].
pub struct CustomBlockInfo {
    pub block: &'static Block,
    pub properties: Box<[CustomProperty]>,
    /// Tag names (`namespace:path`) the block belongs to.
    pub tags: Box<[&'static str]>,
    /// Whether this is a placeholder for a block whose plugin is missing.
    pub placeholder: bool,
    /// The plugin that registered the block, if any.
    pub owner: Option<&'static str>,
    /// Behaviour callbacks the owner handles (bit set, see the server's custom block module).
    pub callbacks: AtomicU32,
    /// Custom block entity type id created with the block, if any.
    pub block_entity: Option<&'static str>,
    /// The server's behaviour handler for this block (set once by the server, opaque here).
    pub handler: OnceLock<Box<dyn std::any::Any + Send + Sync>>,
}

impl fmt::Debug for CustomBlockInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomBlockInfo")
            .field("name", &self.block.name)
            .field("properties", &self.properties)
            .field("tags", &self.tags)
            .field("placeholder", &self.placeholder)
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

/// Extra data of a custom state, next to its [`BlockState`].
#[derive(Debug)]
pub struct CustomStateInfo {
    pub block: BlockId,
    /// The vanilla state clients are shown.
    pub client_state: BlockStateId,
    /// Value index for each property of the block.
    pub values: Box<[u8]>,
    pub blast_resistance: f32,
    pub map_color: u8,
    pub loot_table: Option<&'static str>,
    pub sounds: Option<CustomSoundGroup>,
}

impl CustomBlockInfo {
    /// Whether the block carries the tag `name` (`minecraft:` may be omitted).
    #[must_use]
    pub fn has_tag_name(&self, name: &str) -> bool {
        let name = name.strip_prefix('#').unwrap_or(name);
        self.tags.iter().any(|tag| {
            *tag == name || (!name.contains(':') && tag.strip_prefix("minecraft:") == Some(name))
        })
    }

    /// The state of this block with the given property values (by value index).
    #[must_use]
    pub fn state_with_values(&self, values: &[u8]) -> Option<BlockStateId> {
        if values.len() != self.properties.len() {
            return None;
        }
        let mut index = 0usize;
        for (property, &value) in self.properties.iter().zip(values) {
            let count = property.values.len();
            if usize::from(value) >= count {
                return None;
            }
            index = index * count + usize::from(value);
        }
        self.block.states.get(index).map(|state| state.id)
    }
}

/// Collision or outline shape of a custom state.
#[derive(Debug, Clone)]
pub enum CustomShape {
    Empty,
    FullCube,
    Boxes(Vec<BoundingBox>),
}

impl CustomShape {
    fn boxes(&self) -> Vec<BoundingBox> {
        match self {
            Self::Empty => Vec::new(),
            Self::FullCube => vec![BoundingBox::full_block()],
            Self::Boxes(boxes) => boxes.clone(),
        }
    }
}

/// How a plugin describes one state of a custom block.
#[derive(Debug, Clone)]
pub struct StateDefinition {
    /// The vanilla state clients are shown (required).
    pub client_state: BlockStateId,
    pub hardness: f32,
    pub blast_resistance: f32,
    pub luminance: u8,
    /// Light the state absorbs, 0-15. `None`: 15 for an occluding full cube, else 0.
    pub opacity: Option<u8>,
    pub collision: CustomShape,
    /// `None`: same as `collision`.
    pub outline: Option<CustomShape>,
    pub piston_behavior: PistonBehavior,
    /// Vanilla sound group by its `SoundType` name (`STONE`, `WOOD`, ...).
    pub sound_group: String,
    /// Resource pack sounds that replace the vanilla group's sounds.
    pub custom_sounds: Option<CustomSoundGroup>,
    pub instrument: NoteblockInstrument,
    pub map_color: u8,
    /// Loot table id dropped when broken; `None` drops nothing.
    pub loot_table: Option<String>,
    /// Vanilla `requiresCorrectToolForDrops`.
    pub tool_required: bool,
    pub replaceable: bool,
    pub burnable: bool,
    pub random_ticks: bool,
    /// Redstone conductor (`isRedstoneConductor`). `None`: occluding full cube.
    pub redstone_conductor: Option<bool>,
    /// Occludes light and faces (`canOcclude`). `None`: full cube collision.
    pub occludes: Option<bool>,
}

impl Default for StateDefinition {
    fn default() -> Self {
        Self {
            client_state: Block::STONE.default_state.id,
            hardness: 1.5,
            blast_resistance: 6.0,
            luminance: 0,
            opacity: None,
            collision: CustomShape::FullCube,
            outline: None,
            piston_behavior: PistonBehavior::Normal,
            sound_group: "STONE".to_string(),
            custom_sounds: None,
            instrument: NoteblockInstrument::Harp,
            map_color: 11,
            loot_table: None,
            tool_required: false,
            replaceable: false,
            burnable: false,
            random_ticks: false,
            redstone_conductor: None,
            occludes: None,
        }
    }
}

/// How a plugin describes a custom block.
#[derive(Debug, Clone, Default)]
pub struct BlockDefinition {
    /// `namespace:path`, not in the `minecraft` namespace.
    pub name: String,
    /// Property names and their values. States are the cartesian product in vanilla order
    /// (the last property changes fastest).
    pub properties: Vec<(String, Vec<String>)>,
    /// Index of the default state.
    pub default_state: u16,
    /// One definition per state, or a single one used for every state.
    pub states: Vec<StateDefinition>,
    pub slipperiness: Option<f32>,
    pub velocity_multiplier: Option<f32>,
    pub jump_velocity_multiplier: Option<f32>,
    /// Item id of the block's item form (0: none).
    pub item_id: u16,
    /// Fire spread and burn chances.
    pub flammable: Option<(u8, u8)>,
    pub tags: Vec<String>,
    pub owner: Option<String>,
    /// Custom block entity type created with the block.
    pub block_entity: Option<String>,
}

/// Why a registration was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterError {
    /// The server finished loading plugins; the registry is fixed.
    Frozen,
    InvalidName(String),
    /// The name is a vanilla block or was registered before.
    Duplicate(String),
    InvalidProperties(String),
    /// The number of state definitions does not match the properties.
    StateCount {
        expected: usize,
        got: usize,
    },
    /// A client state is not a vanilla state.
    InvalidClientState(u16),
    UnknownSoundGroup(String),
    TooManyStates,
    TooManyBlocks,
    TooManyShapes,
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frozen => write!(f, "custom blocks can only be registered while plugins load"),
            Self::InvalidName(name) => write!(f, "invalid custom block name `{name}`"),
            Self::Duplicate(name) => write!(f, "block `{name}` already exists"),
            Self::InvalidProperties(reason) => write!(f, "invalid properties: {reason}"),
            Self::StateCount { expected, got } => {
                write!(f, "expected 1 or {expected} state definitions, got {got}")
            }
            Self::InvalidClientState(id) => write!(f, "client state {id} is not a vanilla state"),
            Self::UnknownSoundGroup(name) => write!(f, "unknown sound group `{name}`"),
            Self::TooManyStates => write!(f, "no free block state ids left"),
            Self::TooManyBlocks => write!(f, "no free block ids left"),
            Self::TooManyShapes => write!(f, "too many distinct custom shapes"),
        }
    }
}

impl std::error::Error for RegisterError {}

// --- lookups (lock free) ---

/// Whether `id` is a custom (non-vanilla) state.
#[inline]
#[must_use]
pub const fn is_custom_state(id: BlockStateId) -> bool {
    id.as_u16() >= VANILLA_STATE_COUNT
}

/// Whether `id` is a custom (non-vanilla) block.
#[inline]
#[must_use]
pub const fn is_custom_block(id: BlockId) -> bool {
    id.as_u16() >= VANILLA_BLOCK_COUNT
}

/// Whether any custom state (including placeholders) exists.
#[inline]
#[must_use]
pub fn any_registered() -> bool {
    STATE_COUNT.load(Ordering::Relaxed) != 0
}

/// Vanilla plus custom states.
#[inline]
#[must_use]
pub fn total_state_count() -> u16 {
    VANILLA_STATE_COUNT + STATE_COUNT.load(Ordering::Acquire)
}

/// Vanilla plus custom blocks.
#[inline]
#[must_use]
pub fn total_block_count() -> u16 {
    VANILLA_BLOCK_COUNT + BLOCK_COUNT.load(Ordering::Acquire)
}

#[inline]
pub(crate) fn is_valid_custom_state(raw: u16) -> bool {
    raw >= VANILLA_STATE_COUNT && raw - VANILLA_STATE_COUNT < STATE_COUNT.load(Ordering::Acquire)
}

#[inline]
pub(crate) fn is_valid_custom_block(raw: u16) -> bool {
    raw >= VANILLA_BLOCK_COUNT && raw - VANILLA_BLOCK_COUNT < BLOCK_COUNT.load(Ordering::Acquire)
}

/// The state behind a custom id. `id` must be a valid custom state.
#[inline]
#[must_use]
pub(crate) fn state(id: BlockStateId) -> &'static BlockState {
    let slot = usize::from(id.as_u16() - VANILLA_STATE_COUNT);
    // SAFETY: a custom `BlockStateId` is only constructed below the published count, and every
    // slot below it holds a leaked, never freed state.
    unsafe { &*STATES[slot].load(Ordering::Acquire) }
}

/// The block behind a custom id. `id` must be a valid custom block.
#[inline]
#[must_use]
pub(crate) fn block(id: BlockId) -> &'static Block {
    let slot = usize::from(id.as_u16() - VANILLA_BLOCK_COUNT);
    // SAFETY: as in `state`.
    unsafe { &*BLOCKS[slot].load(Ordering::Acquire) }
}

/// Extra data of a custom state, `None` for vanilla states.
#[inline]
#[must_use]
pub fn state_info(id: BlockStateId) -> Option<&'static CustomStateInfo> {
    let slot = usize::from(id.as_u16().checked_sub(VANILLA_STATE_COUNT)?);
    let ptr = STATE_INFOS.get(slot)?.load(Ordering::Acquire);
    // SAFETY: filled slots hold leaked, never freed data.
    (!ptr.is_null()).then(|| unsafe { &*ptr })
}

/// Extra data of a custom block, `None` for vanilla blocks.
#[inline]
#[must_use]
pub fn block_info(id: BlockId) -> Option<&'static CustomBlockInfo> {
    let slot = usize::from(id.as_u16().checked_sub(VANILLA_BLOCK_COUNT)?);
    let ptr = BLOCK_INFOS.get(slot)?.load(Ordering::Acquire);
    // SAFETY: filled slots hold leaked, never freed data.
    (!ptr.is_null()).then(|| unsafe { &*ptr })
}

/// [`block_info`] of the block a state belongs to.
#[inline]
#[must_use]
pub fn block_info_of_state(id: BlockStateId) -> Option<&'static CustomBlockInfo> {
    block_info(state_info(id)?.block)
}

#[inline]
pub(crate) fn block_id_of_state(id: BlockStateId) -> BlockId {
    state_info(id).map_or(BlockId::AIR, |info| info.block)
}

/// The vanilla state clients are shown for `id` (`id` itself for vanilla states).
#[inline]
#[must_use]
pub fn client_state(id: BlockStateId) -> BlockStateId {
    if !is_custom_state(id) {
        return id;
    }
    state_info(id).map_or(BlockStateId::AIR, |info| info.client_state)
}

/// A collision shape by index; indices past the vanilla table are custom shapes.
#[inline]
#[must_use]
pub(crate) fn shape(index: u16) -> BoundingBox {
    let index = usize::from(index);
    if let Some(shape) = COLLISION_SHAPES.get(index) {
        return *shape;
    }
    let ptr = SHAPES
        .get(index - COLLISION_SHAPES.len())
        .map_or(std::ptr::null_mut(), |slot| slot.load(Ordering::Acquire));
    if ptr.is_null() {
        return BoundingBox::full_block();
    }
    // SAFETY: filled slots hold leaked, never freed shapes.
    unsafe { *ptr }
}

/// A real (non placeholder) custom block by `namespace:path`.
#[must_use]
pub fn block_by_name(name: &str) -> Option<&'static Block> {
    if !any_registered() {
        return None;
    }
    let id = if let Some(names) = FROZEN_NAMES.get() {
        *names.get(name)?
    } else {
        *writer().names.as_ref()?.get(name)?
    };
    Some(block(id))
}

/// Every real custom block, in id order.
#[must_use]
pub fn blocks() -> Vec<&'static CustomBlockInfo> {
    (VANILLA_BLOCK_COUNT..total_block_count())
        .filter_map(|raw| block_info(BlockId::new(raw)?))
        .filter(|info| !info.placeholder)
        .collect()
}

/// The property names and values of a custom state, in property order.
#[must_use]
pub fn state_properties(id: BlockStateId) -> Vec<(&'static str, &'static str)> {
    let Some(info) = state_info(id) else {
        return Vec::new();
    };
    let Some(block) = block_info(info.block) else {
        return Vec::new();
    };
    block
        .properties
        .iter()
        .zip(info.values.iter())
        .map(|(property, &value)| (property.name, property.values[usize::from(value)]))
        .collect()
}

/// `id` with property `key` set to `value`, if the block has that property and value.
#[must_use]
pub fn with_property(id: BlockStateId, key: &str, value: &str) -> Option<BlockStateId> {
    let info = state_info(id)?;
    let block = block_info(info.block)?;
    let property = block.properties.iter().position(|p| p.name == key)?;
    let value = block.properties[property]
        .values
        .iter()
        .position(|v| *v == value)?;
    let mut values = info.values.to_vec();
    values[property] = u8::try_from(value).ok()?;
    block.state_with_values(&values)
}

/// The registry name (`minecraft:stone`, `ns:path`) and property strings of any state.
#[must_use]
pub fn state_name_and_properties(id: BlockStateId) -> (String, Vec<(&'static str, &'static str)>) {
    let block = Block::from_state_id(id);
    if is_custom_state(id) {
        return (block.name.to_string(), state_properties(id));
    }
    let properties = block
        .properties(id)
        .map(|props| {
            props
                .to_props()
                .into_iter()
                .map(|p| (p.key_str(), p.value_str()))
                .collect()
        })
        .unwrap_or_default();
    (format!("minecraft:{}", block.name), properties)
}

/// Resolves a state from a block name and property strings, vanilla or custom. Unknown
/// properties are ignored and missing ones keep the default state's value, like vanilla.
/// `None` when no block has that name.
pub fn resolve_state<'a>(
    name: &str,
    properties: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Option<BlockStateId> {
    let block = Block::from_name(name)?;
    let mut properties = properties.into_iter().peekable();
    if properties.peek().is_none() {
        return Some(block.default_state.id);
    }
    if is_custom_block(block.id) {
        let mut state = block.default_state.id;
        for (key, value) in properties {
            if let Some(next) = with_property(state, key, value) {
                state = next;
            }
        }
        return Some(state);
    }
    let properties: Vec<crate::Property> = properties
        .filter_map(|(key, value)| crate::Property::from_str_pair(key, value))
        .collect();
    if properties.is_empty() || block.properties(block.default_state.id).is_none() {
        return Some(block.default_state.id);
    }
    Some(block.apply_properties(block.default_state.id, &properties))
}

/// Like [`resolve_state`], but an unknown block name gives a placeholder that keeps the name and
/// properties (for chunk data whose plugin is missing).
pub fn resolve_state_or_placeholder(name: &str, properties: &[(&str, &str)]) -> BlockStateId {
    if let Some(state) = resolve_state(name, properties.iter().copied()) {
        return state;
    }
    placeholder(name, properties).unwrap_or(BlockStateId::AIR)
}

// --- registration ---

/// Whether plugins may still register blocks.
#[must_use]
pub fn is_frozen() -> bool {
    FROZEN.load(Ordering::Acquire)
}

/// Closes registration. Called by the server once plugins are loaded, before worlds load.
pub fn freeze() {
    let mut writer = writer();
    if FROZEN.swap(true, Ordering::AcqRel) {
        return;
    }
    let names = writer.names.take().unwrap_or_default();
    let _ = FROZEN_NAMES.set(names);
}

fn valid_identifier(name: &str) -> bool {
    let Some((namespace, path)) = name.split_once(':') else {
        return false;
    };
    !namespace.is_empty()
        && !path.is_empty()
        && namespace
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'))
        && path
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'))
}

fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn sound_group_index(name: &str) -> Option<u8> {
    SOUND_TYPES
        .iter()
        .position(|group| group.name.eq_ignore_ascii_case(name))
        .and_then(|index| u8::try_from(index).ok())
}

fn shape_key(shape: &BoundingBox) -> [u64; 6] {
    [
        shape.min.x.to_bits(),
        shape.min.y.to_bits(),
        shape.min.z.to_bits(),
        shape.max.x.to_bits(),
        shape.max.y.to_bits(),
        shape.max.z.to_bits(),
    ]
}

/// Index of `shape` in the shape table, adding it after the vanilla shapes if needed.
fn shape_index(writer: &mut Writer, shape: &BoundingBox) -> Result<u16, RegisterError> {
    if let Some(index) = COLLISION_SHAPES
        .iter()
        .position(|s| shape_key(s) == shape_key(shape))
    {
        return u16::try_from(index).map_err(|_| RegisterError::TooManyShapes);
    }
    let key = shape_key(shape);
    let shapes = writer.shapes.get_or_insert_with(HashMap::new);
    if let Some(&index) = shapes.get(&key) {
        return Ok(index);
    }
    let slot = usize::from(writer.shape_count);
    if slot >= SHAPE_CAPACITY {
        return Err(RegisterError::TooManyShapes);
    }
    let index =
        u16::try_from(COLLISION_SHAPES.len() + slot).map_err(|_| RegisterError::TooManyShapes)?;
    SHAPES[slot].store(Box::into_raw(Box::new(*shape)), Ordering::Release);
    writer.shape_count += 1;
    writer
        .shapes
        .get_or_insert_with(HashMap::new)
        .insert(key, index);
    Ok(index)
}

fn shape_indices(
    writer: &mut Writer,
    boxes: &[BoundingBox],
) -> Result<&'static [u16], RegisterError> {
    let mut indices = Vec::with_capacity(boxes.len());
    for shape in boxes {
        indices.push(shape_index(writer, shape)?);
    }
    Ok(Box::leak(indices.into_boxed_slice()))
}

const EPS: f64 = 1.0e-7;

fn is_full_cube(boxes: &[BoundingBox]) -> bool {
    boxes.iter().any(|b| {
        b.min.x <= EPS
            && b.min.y <= EPS
            && b.min.z <= EPS
            && b.max.x >= 1.0 - EPS
            && b.max.y >= 1.0 - EPS
            && b.max.z >= 1.0 - EPS
    })
}

/// Vanilla `isFaceSturdy` (FULL) and the CENTER support check, per box: a face is sturdy when one
/// box covers it, center-solid when one box covers the 2x2 pixel center of the top or bottom.
fn side_flags(boxes: &[BoundingBox]) -> u8 {
    let covers = |lo: f64, hi: f64, a: f64, b: f64| lo <= a + EPS && hi >= b - EPS;
    let mut out = 0u8;
    for b in boxes {
        let full_xz = covers(b.min.x, b.max.x, 0.0, 1.0) && covers(b.min.z, b.max.z, 0.0, 1.0);
        let full_xy = covers(b.min.x, b.max.x, 0.0, 1.0) && covers(b.min.y, b.max.y, 0.0, 1.0);
        let full_zy = covers(b.min.z, b.max.z, 0.0, 1.0) && covers(b.min.y, b.max.y, 0.0, 1.0);
        let center_xz = covers(b.min.x, b.max.x, 7.0 / 16.0, 9.0 / 16.0)
            && covers(b.min.z, b.max.z, 7.0 / 16.0, 9.0 / 16.0);
        if b.min.y <= EPS {
            if full_xz {
                out |= flags::DOWN_SIDE_SOLID;
            }
            if center_xz {
                out |= flags::DOWN_CENTER_SOLID;
            }
        }
        if b.max.y >= 1.0 - EPS {
            if full_xz {
                out |= flags::UP_SIDE_SOLID;
            }
            if center_xz {
                out |= flags::UP_CENTER_SOLID;
            }
        }
        if b.min.z <= EPS && full_xy {
            out |= flags::NORTH_SIDE_SOLID;
        }
        if b.max.z >= 1.0 - EPS && full_xy {
            out |= flags::SOUTH_SIDE_SOLID;
        }
        if b.min.x <= EPS && full_zy {
            out |= flags::WEST_SIDE_SOLID;
        }
        if b.max.x >= 1.0 - EPS && full_zy {
            out |= flags::EAST_SIDE_SOLID;
        }
    }
    out
}

/// Vanilla's legacy `isSolid` (`calculateSolid`): a collision shape whose bounds average at
/// least 0.7291666 or are a full block tall.
fn legacy_solid(boxes: &[BoundingBox]) -> bool {
    if boxes.is_empty() {
        return false;
    }
    let mut min = Vector3::new(f64::MAX, f64::MAX, f64::MAX);
    let mut max = Vector3::new(f64::MIN, f64::MIN, f64::MIN);
    for b in boxes {
        min = Vector3::new(min.x.min(b.min.x), min.y.min(b.min.y), min.z.min(b.min.z));
        max = Vector3::new(max.x.max(b.max.x), max.y.max(b.max.y), max.z.max(b.max.z));
    }
    let size = (max.x - min.x + max.y - min.y + max.z - min.z) / 3.0;
    size >= 0.729_166_666_666_666_6 || max.y - min.y >= 1.0
}

fn build_state(
    writer: &mut Writer,
    id: BlockStateId,
    def: &StateDefinition,
    has_block_entity: bool,
) -> Result<BlockState, RegisterError> {
    if is_custom_state(def.client_state) {
        return Err(RegisterError::InvalidClientState(def.client_state.as_u16()));
    }
    let sound_type = sound_group_index(&def.sound_group)
        .ok_or_else(|| RegisterError::UnknownSoundGroup(def.sound_group.clone()))?;
    let collision = def.collision.boxes();
    let outline = def
        .outline
        .as_ref()
        .map_or_else(|| collision.clone(), CustomShape::boxes);
    let full_cube = is_full_cube(&collision);
    let occludes = def.occludes.unwrap_or(full_cube);
    let solid_render = occludes && is_full_cube(&outline);
    let solid = !def.replaceable && legacy_solid(&collision);

    let mut state_flags = 0u16;
    let mut set = |flag: u16, on: bool| {
        if on {
            state_flags |= flag;
        }
    };
    set(flags::BURNABLE, def.burnable);
    set(flags::TOOL_REQUIRED, def.tool_required);
    set(flags::REPLACEABLE, def.replaceable);
    set(flags::IS_SOLID, solid);
    set(flags::IS_FULL_CUBE, full_cube);
    set(
        flags::IS_SOLID_BLOCK,
        def.redstone_conductor.unwrap_or(solid_render),
    );
    set(flags::HAS_RANDOM_TICKS, def.random_ticks);
    set(flags::IS_SOLID_RENDER, solid_render);
    set(flags::CAN_OCCLUDE, occludes);

    Ok(BlockState {
        id,
        state_flags,
        side_flags: side_flags(&collision),
        instrument: def.instrument,
        luminance: def.luminance.min(15),
        piston_behavior: def.piston_behavior.clone(),
        hardness: def.hardness,
        collision_shapes: shape_indices(writer, &collision)?,
        outline_shapes: shape_indices(writer, &outline)?,
        opacity: def
            .opacity
            .unwrap_or(if solid_render { 15 } else { 0 })
            .min(15),
        sound_type,
        block_entity_type: if has_block_entity {
            CUSTOM_BLOCK_ENTITY_TYPE
        } else {
            u16::MAX
        },
    })
}

/// Registers a custom block. Only allowed before [`freeze`].
pub fn register(def: &BlockDefinition) -> Result<&'static CustomBlockInfo, RegisterError> {
    let mut writer = writer();
    if FROZEN.load(Ordering::Acquire) {
        return Err(RegisterError::Frozen);
    }
    if !valid_identifier(&def.name) || def.name.starts_with("minecraft:") {
        return Err(RegisterError::InvalidName(def.name.clone()));
    }
    if writer
        .names
        .as_ref()
        .is_some_and(|names| names.contains_key(def.name.as_str()))
    {
        return Err(RegisterError::Duplicate(def.name.clone()));
    }
    let info = insert_block(&mut writer, def, false)?;
    writer
        .names
        .get_or_insert_with(HashMap::new)
        .insert(info.block.name, info.block.id);
    Ok(info)
}

/// A placeholder state for a block read from disk whose name is unknown. The same name and
/// properties always give the same state.
pub fn placeholder(name: &str, properties: &[(&str, &str)]) -> Option<BlockStateId> {
    let mut sorted: Vec<(&str, &str)> = properties.to_vec();
    sorted.sort_unstable();
    let mut key = name.to_string();
    for (k, v) in &sorted {
        key.push('|');
        key.push_str(k);
        key.push('=');
        key.push_str(v);
    }
    let mut writer = writer();
    if let Some(&state) = writer.placeholders.as_ref().and_then(|map| map.get(&key)) {
        return Some(state);
    }
    let def = BlockDefinition {
        name: name.to_string(),
        properties: properties
            .iter()
            .map(|(k, v)| ((*k).to_string(), vec![(*v).to_string()]))
            .collect(),
        states: vec![placeholder_state()],
        ..Default::default()
    };
    let info = insert_block(&mut writer, &def, true).ok()?;
    let state = info.block.default_state.id;
    writer
        .placeholders
        .get_or_insert_with(HashMap::new)
        .insert(key, state);
    Some(state)
}

fn placeholder_state() -> StateDefinition {
    StateDefinition {
        client_state: Block::BEDROCK.default_state.id,
        hardness: -1.0,
        blast_resistance: 3_600_000.0,
        piston_behavior: PistonBehavior::Block,
        ..Default::default()
    }
}

fn insert_block(
    writer: &mut Writer,
    def: &BlockDefinition,
    placeholder: bool,
) -> Result<&'static CustomBlockInfo, RegisterError> {
    if !placeholder && Block::from_vanilla_name(&def.name).is_some() {
        return Err(RegisterError::Duplicate(def.name.clone()));
    }
    // Properties.
    let mut properties = Vec::with_capacity(def.properties.len());
    let mut state_count = 1usize;
    for (name, values) in &def.properties {
        if name.is_empty() || values.is_empty() || values.len() > usize::from(u8::MAX) {
            return Err(RegisterError::InvalidProperties(format!(
                "property `{name}` needs 1 to 255 values"
            )));
        }
        if properties.iter().any(|p: &CustomProperty| p.name == name) {
            return Err(RegisterError::InvalidProperties(format!(
                "property `{name}` appears twice"
            )));
        }
        for (i, value) in values.iter().enumerate() {
            if value.is_empty() || values[..i].contains(value) {
                return Err(RegisterError::InvalidProperties(format!(
                    "property `{name}` has an empty or repeated value"
                )));
            }
        }
        state_count = state_count.saturating_mul(values.len());
        properties.push(CustomProperty {
            name: leak_str(name),
            values: values.iter().map(|v| leak_str(v)).collect(),
        });
    }
    if def.states.len() != 1 && def.states.len() != state_count {
        return Err(RegisterError::StateCount {
            expected: state_count,
            got: def.states.len(),
        });
    }
    if usize::from(def.default_state) >= state_count {
        return Err(RegisterError::InvalidProperties(
            "default state out of range".to_string(),
        ));
    }

    let first_slot = usize::from(STATE_COUNT.load(Ordering::Relaxed));
    if first_slot + state_count > STATE_CAPACITY {
        return Err(RegisterError::TooManyStates);
    }
    let block_slot = usize::from(BLOCK_COUNT.load(Ordering::Relaxed));
    if block_slot >= BLOCK_CAPACITY {
        return Err(RegisterError::TooManyBlocks);
    }
    let block_id = BlockId(VANILLA_BLOCK_COUNT + block_slot as u16);

    // States and their extras.
    let mut states = Vec::with_capacity(state_count);
    let mut infos = Vec::with_capacity(state_count);
    for index in 0..state_count {
        let state_def = &def.states[if def.states.len() == 1 { 0 } else { index }];
        let id = BlockStateId(VANILLA_STATE_COUNT + (first_slot + index) as u16);
        states.push(build_state(
            writer,
            id,
            state_def,
            def.block_entity.is_some(),
        )?);
        // Value index of each property (last property changes fastest).
        let mut values = vec![0u8; properties.len()];
        let mut rest = index;
        for (slot, property) in properties.iter().enumerate().rev() {
            let count = property.values.len();
            values[slot] = (rest % count) as u8;
            rest /= count;
        }
        infos.push(CustomStateInfo {
            block: block_id,
            client_state: state_def.client_state,
            values: values.into_boxed_slice(),
            blast_resistance: state_def.blast_resistance,
            map_color: state_def.map_color,
            loot_table: state_def.loot_table.as_deref().map(leak_str),
            sounds: state_def.custom_sounds.clone(),
        });
    }
    let states: &'static [BlockState] = Box::leak(states.into_boxed_slice());
    let infos: &'static [CustomStateInfo] = Box::leak(infos.into_boxed_slice());
    let default_state = &states[usize::from(def.default_state)];
    let default_info = &infos[usize::from(def.default_state)];

    let block: &'static Block = Box::leak(Box::new(Block {
        id: block_id,
        name: leak_str(&def.name),
        hardness: default_state.hardness,
        blast_resistance: default_info.blast_resistance,
        map_color: default_info.map_color,
        slipperiness: def.slipperiness.unwrap_or(0.6),
        velocity_multiplier: def.velocity_multiplier.unwrap_or(1.0),
        jump_velocity_multiplier: def.jump_velocity_multiplier.unwrap_or(1.0),
        bounce_restitution: 0.0,
        fall_distance_reduction: 0.0,
        item_id: def.item_id,
        default_state,
        states,
        flammable: def.flammable.map(|(spread_chance, burn_chance)| Flammable {
            spread_chance,
            burn_chance,
        }),
        experience: None,
        loot_table: default_info.loot_table,
    }));
    let info: &'static CustomBlockInfo = Box::leak(Box::new(CustomBlockInfo {
        block,
        properties: properties.into_boxed_slice(),
        tags: def
            .tags
            .iter()
            .map(|tag| {
                let tag = tag.strip_prefix('#').unwrap_or(tag);
                if tag.contains(':') {
                    leak_str(tag)
                } else {
                    leak_str(&format!("minecraft:{tag}"))
                }
            })
            .collect(),
        placeholder,
        owner: def.owner.as_deref().map(leak_str),
        callbacks: AtomicU32::new(0),
        block_entity: def.block_entity.as_deref().map(leak_str),
        handler: OnceLock::new(),
    }));

    // Publish: slots first, then the counts.
    for (index, (state, extra)) in states.iter().zip(infos).enumerate() {
        STATE_INFOS[first_slot + index]
            .store(std::ptr::from_ref(extra).cast_mut(), Ordering::Release);
        STATES[first_slot + index].store(std::ptr::from_ref(state).cast_mut(), Ordering::Release);
    }
    BLOCK_INFOS[block_slot].store(std::ptr::from_ref(info).cast_mut(), Ordering::Release);
    BLOCKS[block_slot].store(std::ptr::from_ref(block).cast_mut(), Ordering::Release);
    BLOCK_COUNT.store(block_slot as u16 + 1, Ordering::Release);
    STATE_COUNT.store((first_slot + state_count) as u16, Ordering::Release);
    Ok(info)
}

/// Serializes tests that touch the process-wide registry (also from other crates).
#[doc(hidden)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Registers a test block once per process (tests share the registry and it never shrinks).
#[doc(hidden)]
pub fn test_block() -> &'static CustomBlockInfo {
    static BLOCK: OnceLock<&'static CustomBlockInfo> = OnceLock::new();
    BLOCK.get_or_init(|| {
        if let Some(block) = block_by_name("pumpkin_test:ruby_ore") {
            return block_info(block.id).unwrap();
        }
        let lit = StateDefinition {
            client_state: Block::NOTE_BLOCK.states[1].id,
            hardness: 3.0,
            luminance: 9,
            tool_required: true,
            ..Default::default()
        };
        let unlit = StateDefinition {
            client_state: Block::NOTE_BLOCK.states[0].id,
            hardness: 3.0,
            tool_required: true,
            ..Default::default()
        };
        register(&BlockDefinition {
            name: "pumpkin_test:ruby_ore".to_string(),
            properties: vec![(
                "lit".to_string(),
                vec!["true".to_string(), "false".to_string()],
            )],
            default_state: 1,
            states: vec![lit, unlit],
            tags: vec![
                "mineable/pickaxe".to_string(),
                "minecraft:needs_iron_tool".to_string(),
            ],
            ..Default::default()
        })
        .expect("test block registers")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::{self, Taggable};

    #[test]
    fn registers_and_looks_up_custom_blocks() {
        let _guard = TEST_LOCK.lock().unwrap();
        let info = test_block();
        let block = info.block;
        assert!(is_custom_block(block.id));
        assert_eq!(block.states.len(), 2);
        let lit = block.states[0].id;
        let unlit = block.states[1].id;
        assert!(is_custom_state(lit));
        assert_eq!(block.default_state.id, unlit);

        // Every lookup path reaches the same leaked data.
        assert!(std::ptr::eq(Block::from_state_id(lit), block));
        assert!(std::ptr::eq(Block::from_id(block.id), block));
        assert!(std::ptr::eq(BlockState::from_id(lit), &block.states[0]));
        assert_eq!(BlockStateId::new(lit.as_u16()), Some(lit));
        assert_eq!(BlockId::new(block.id.as_u16()), Some(block.id));
        assert!(std::ptr::eq(
            Block::from_name("pumpkin_test:ruby_ore").unwrap(),
            block
        ));

        // Per-state attributes.
        assert_eq!(lit.to_state().luminance, 9);
        assert_eq!(unlit.to_state().luminance, 0);
        assert!(lit.to_state().tool_required());
        assert!(lit.to_state().is_full_cube());
        assert!(lit.to_state().is_solid());
        assert!(lit.to_state().is_side_solid(crate::BlockDirection::Up));
        assert_eq!(lit.to_state().opacity, 15);
        assert_eq!(lit.to_state().get_block_collision_shapes().count(), 1);

        // Properties.
        assert_eq!(state_properties(lit), vec![("lit", "true")]);
        assert_eq!(with_property(unlit, "lit", "true"), Some(lit));
        assert_eq!(with_property(unlit, "lit", "maybe"), None);
        assert_eq!(
            resolve_state("pumpkin_test:ruby_ore", [("lit", "true"), ("bogus", "1")]),
            Some(lit)
        );
        let (name, props) = state_name_and_properties(lit);
        assert_eq!(name, "pumpkin_test:ruby_ore");
        assert_eq!(props, vec![("lit", "true")]);

        // Client view and tags.
        assert_eq!(client_state(lit), Block::NOTE_BLOCK.states[1].id);
        assert!(block.has_tag(&tag::Block::MINECRAFT_MINEABLE_PICKAXE));
        assert!(block.has_tag(&tag::Block::MINECRAFT_NEEDS_IRON_TOOL));
        assert!(!block.has_tag(&tag::Block::MINECRAFT_MINEABLE_AXE));
        assert_eq!(
            block.is_tagged_with("minecraft:mineable/pickaxe"),
            Some(true)
        );
    }

    #[test]
    fn vanilla_lookups_are_unchanged() {
        assert_eq!(
            BlockStateId::new(VANILLA_STATE_COUNT - 1).map(|id| id.as_u16()),
            Some(VANILLA_STATE_COUNT - 1)
        );
        assert!(!is_custom_state(Block::STONE.default_state.id));
        assert_eq!(
            client_state(Block::STONE.default_state.id),
            Block::STONE.default_state.id
        );
        assert!(state_info(Block::STONE.default_state.id).is_none());
        let (name, props) = state_name_and_properties(Block::OAK_STAIRS.default_state.id);
        assert_eq!(name, "minecraft:oak_stairs");
        assert!(props.contains(&("facing", "north")));
        assert_eq!(
            resolve_state("minecraft:oak_stairs", props.iter().copied()),
            Some(Block::OAK_STAIRS.default_state.id)
        );
        assert_eq!(
            resolve_state("stone", []),
            Some(Block::STONE.default_state.id)
        );
        assert_eq!(resolve_state("nope:nothing", []), None);
    }

    #[test]
    fn placeholders_keep_name_and_properties() {
        let _guard = TEST_LOCK.lock().unwrap();
        let props = [("facing", "east"), ("variant", "7")];
        let a = resolve_state_or_placeholder("gone_plugin:thing", &props);
        let b = resolve_state_or_placeholder(
            "gone_plugin:thing",
            &[("variant", "7"), ("facing", "east")],
        );
        assert_eq!(a, b);
        let other = resolve_state_or_placeholder("gone_plugin:thing", &[("facing", "west")]);
        assert_ne!(a, other);
        let (name, read_back) = state_name_and_properties(a);
        assert_eq!(name, "gone_plugin:thing");
        assert_eq!(read_back, props.to_vec());
        assert!(block_info_of_state(a).unwrap().placeholder);
        // Placeholders are not real blocks: commands and `from_name` do not see them.
        assert!(Block::from_name("gone_plugin:thing").is_none());
        assert_eq!(client_state(a), Block::BEDROCK.default_state.id);
        assert!(a.to_state().hardness < 0.0);
    }

    #[test]
    fn rejects_bad_definitions() {
        let _guard = TEST_LOCK.lock().unwrap();
        let _ = test_block();
        let base = BlockDefinition {
            name: "pumpkin_test:bad".to_string(),
            states: vec![StateDefinition::default()],
            ..Default::default()
        };
        let mut def = base.clone();
        def.name = "minecraft:stone2".to_string();
        assert!(matches!(register(&def), Err(RegisterError::InvalidName(_))));
        let mut def = base.clone();
        def.name = "pumpkin_test:ruby_ore".to_string();
        assert!(matches!(register(&def), Err(RegisterError::Duplicate(_))));
        let mut def = base.clone();
        def.states[0].client_state = test_block().block.default_state.id;
        assert!(matches!(
            register(&def),
            Err(RegisterError::InvalidClientState(_))
        ));
        let mut def = base.clone();
        def.properties = vec![("a".to_string(), vec!["x".to_string(), "y".to_string()])];
        def.states = vec![StateDefinition::default(); 3];
        assert!(matches!(
            register(&def),
            Err(RegisterError::StateCount { .. })
        ));
        let mut def = base;
        def.states[0].sound_group = "NOPE".to_string();
        assert!(matches!(
            register(&def),
            Err(RegisterError::UnknownSoundGroup(_))
        ));
    }

    #[test]
    fn custom_shapes_are_appended() {
        let mut writer = writer();
        let slab = BoundingBox::new_array([0.0, 0.0, 0.0], [1.0, 0.5, 1.0]);
        // A vanilla shape reuses the vanilla index.
        assert!(usize::from(shape_index(&mut writer, &slab).unwrap()) < COLLISION_SHAPES.len());
        let odd = BoundingBox::new_array([0.1, 0.0, 0.1], [0.9, 0.3333, 0.9]);
        let index = shape_index(&mut writer, &odd).unwrap();
        assert!(usize::from(index) >= COLLISION_SHAPES.len());
        assert_eq!(shape_index(&mut writer, &odd).unwrap(), index);
        drop(writer);
        assert_eq!(shape_key(&shape(index)), shape_key(&odd));
        assert_eq!(
            side_flags(&[slab]) & flags::DOWN_SIDE_SOLID,
            flags::DOWN_SIDE_SOLID
        );
        assert_eq!(side_flags(&[slab]) & flags::UP_SIDE_SOLID, 0);
        assert!(!legacy_solid(&[]));
        assert!(legacy_solid(&[slab]));
    }
}
