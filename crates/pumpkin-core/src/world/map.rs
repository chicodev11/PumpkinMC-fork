//! Vanilla maps: `MapItemSavedData` (with its `HoldingPlayer`s, banners and frames), the
//! `MapItem.update` exploration renderer, and `data/minecraft/maps/<id>.dat` storage.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};

use dashmap::DashMap;
use indexmap::IndexMap;
use pumpkin_data::{
    Block, BlockDirection, BlockStateId, dimension::Dimension, map_color::MapColor,
    map_decoration::MapDecorationType,
};
use pumpkin_nbt::{
    compound::NbtCompound,
    nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag},
    tag::NbtTag,
};
use pumpkin_protocol::{
    codec::var_int::VarInt,
    java::client::play::{CMapItemData, MapIcon, MapPatch},
};
use pumpkin_util::{
    math::{position::BlockPos, vector2::Vector2},
    text::TextComponent,
};
use tracing::warn;
use uuid::Uuid;

use crate::{entity::player::Player, world::World};

pub const MAP_SIZE: usize = 128;
const DATA_VERSION: i32 = 5023;

/// Vanilla `MapDecoration`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoration {
    pub kind: &'static MapDecorationType,
    pub x: i8,
    pub y: i8,
    pub rot: i8,
    pub name: Option<TextComponent>,
}

/// Vanilla `MapBanner`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapBanner {
    pub pos: BlockPos,
    /// Dye color name, `white` by default.
    pub color: String,
    pub name: Option<String>,
}

impl MapBanner {
    fn id(&self) -> String {
        format!("banner-{},{},{}", self.pos.0.x, self.pos.0.y, self.pos.0.z)
    }

    fn decoration(&self) -> &'static MapDecorationType {
        MapDecorationType::from_name(&format!("banner_{}", self.color))
            .unwrap_or(&MapDecorationType::BANNER_WHITE)
    }

    /// Vanilla `MapBanner.fromWorld`: the banner block entity's base color and custom name.
    fn from_world(world: &World, pos: &BlockPos) -> Option<Self> {
        let entity = world.get_block_entity(pos)?;
        let banner = entity
            .as_any()
            .downcast_ref::<crate::block::entities::banner::BannerBlockEntity>()?;
        let block = world.get_block(pos);
        let color = block
            .name
            .strip_suffix("_wall_banner")
            .or_else(|| block.name.strip_suffix("_banner"))?
            .to_string();
        let name = banner
            .custom_name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        Some(Self {
            pos: *pos,
            color,
            name,
        })
    }

    fn to_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put(
            "pos",
            NbtTag::IntArray(vec![self.pos.0.x, self.pos.0.y, self.pos.0.z]),
        );
        nbt.put_string("color", self.color.clone());
        if let Some(name) = &self.name {
            nbt.put_string("name", name.clone());
        }
        nbt
    }

    fn from_nbt(nbt: &NbtCompound) -> Option<Self> {
        let pos = nbt.get_int_array("pos")?;
        if pos.len() != 3 {
            return None;
        }
        Some(Self {
            pos: BlockPos::new(pos[0], pos[1], pos[2]),
            color: nbt.get_string("color").unwrap_or("white").to_string(),
            name: nbt.get_string("name").map(ToString::to_string),
        })
    }
}

/// Vanilla `MapFrame`: an item frame showing this map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapFrame {
    pub pos: BlockPos,
    pub rotation: i32,
    pub entity_id: i32,
}

impl MapFrame {
    fn to_nbt(self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put(
            "pos",
            NbtTag::IntArray(vec![self.pos.0.x, self.pos.0.y, self.pos.0.z]),
        );
        nbt.put_int("rotation", self.rotation);
        nbt.put_int("entity_id", self.entity_id);
        nbt
    }

    fn from_nbt(nbt: &NbtCompound) -> Option<Self> {
        let pos = nbt.get_int_array("pos")?;
        if pos.len() != 3 {
            return None;
        }
        Some(Self {
            pos: BlockPos::new(pos[0], pos[1], pos[2]),
            rotation: nbt.get_int("rotation").unwrap_or(0),
            entity_id: nbt.get_int("entity_id").unwrap_or(0),
        })
    }
}

fn frame_id(pos: &BlockPos) -> String {
    format!("frame-{},{},{}", pos.0.x, pos.0.y, pos.0.z)
}

fn frame_key(entity_id: i32) -> String {
    format!("frame-{entity_id}")
}

/// An item frame passed to [`MapData::tick_carried_by`].
#[derive(Clone, Copy, Debug)]
pub struct FrameRef {
    pub pos: BlockPos,
    /// `Direction.get2DDataValue`.
    pub direction_2d: i32,
    pub entity_id: i32,
}

/// Vanilla `MapItemSavedData$HoldingPlayer`: what a player still has to be sent.
struct HoldingPlayer {
    uuid: Uuid,
    dirty_data: bool,
    min_dirty_x: usize,
    min_dirty_y: usize,
    max_dirty_x: usize,
    max_dirty_y: usize,
    dirty_decorations: bool,
    tick: u32,
    step: u32,
}

impl HoldingPlayer {
    const fn new(uuid: Uuid) -> Self {
        Self {
            uuid,
            dirty_data: true,
            min_dirty_x: 0,
            min_dirty_y: 0,
            max_dirty_x: 127,
            max_dirty_y: 127,
            dirty_decorations: true,
            tick: 0,
            step: 0,
        }
    }

    fn mark_colors_dirty(&mut self, x: usize, y: usize) {
        if self.dirty_data {
            self.min_dirty_x = self.min_dirty_x.min(x);
            self.min_dirty_y = self.min_dirty_y.min(y);
            self.max_dirty_x = self.max_dirty_x.max(x);
            self.max_dirty_y = self.max_dirty_y.max(y);
        } else {
            self.dirty_data = true;
            self.min_dirty_x = x;
            self.min_dirty_y = y;
            self.max_dirty_x = x;
            self.max_dirty_y = y;
        }
    }
}

/// Vanilla `MapItemSavedData`.
pub struct MapData {
    pub center_x: i32,
    pub center_z: i32,
    /// Dimension name, `minecraft:overworld`.
    pub dimension: String,
    pub tracking_position: bool,
    pub unlimited_tracking: bool,
    pub scale: i8,
    pub locked: bool,
    pub colors: Box<[u8; MAP_SIZE * MAP_SIZE]>,
    banners: IndexMap<String, MapBanner>,
    frames: IndexMap<String, MapFrame>,
    decorations: IndexMap<String, Decoration>,
    tracked_decoration_count: i32,
    carried_by: Vec<HoldingPlayer>,
    /// Needs saving.
    dirty: bool,
}

impl MapData {
    fn new(
        center_x: i32,
        center_z: i32,
        scale: i8,
        tracking_position: bool,
        unlimited_tracking: bool,
        locked: bool,
        dimension: String,
    ) -> Self {
        Self {
            center_x,
            center_z,
            dimension,
            tracking_position,
            unlimited_tracking,
            scale: scale.clamp(0, 4),
            locked,
            colors: Box::new([0; MAP_SIZE * MAP_SIZE]),
            banners: IndexMap::new(),
            frames: IndexMap::new(),
            decorations: IndexMap::new(),
            tracked_decoration_count: 0,
            carried_by: Vec::new(),
            dirty: true,
        }
    }

    /// Vanilla `createFresh`: the map grid cell around `x`, `z`.
    #[must_use]
    pub fn create_fresh(
        x: f64,
        z: f64,
        scale: i8,
        tracking_position: bool,
        unlimited_tracking: bool,
        dimension: &str,
    ) -> Self {
        let size = 128 * (1 << scale);
        let cell_x = ((x + 64.0) / f64::from(size)).floor() as i32;
        let cell_z = ((z + 64.0) / f64::from(size)).floor() as i32;
        Self::new(
            cell_x * size + size / 2 - 64,
            cell_z * size + size / 2 - 64,
            scale,
            tracking_position,
            unlimited_tracking,
            false,
            dimension.to_string(),
        )
    }

    /// Vanilla `locked`: a locked copy keeping colors, banners and decorations.
    #[must_use]
    pub fn locked_copy(&self) -> Self {
        let mut copy = Self::new(
            self.center_x,
            self.center_z,
            self.scale,
            self.tracking_position,
            self.unlimited_tracking,
            true,
            self.dimension.clone(),
        );
        copy.banners.clone_from(&self.banners);
        copy.decorations.clone_from(&self.decorations);
        copy.tracked_decoration_count = self.tracked_decoration_count;
        copy.colors.copy_from_slice(&self.colors[..]);
        copy
    }

    /// Vanilla `scaled`: a fresh map one scale up around the same center.
    #[must_use]
    pub fn scaled(&self) -> Self {
        Self::create_fresh(
            f64::from(self.center_x),
            f64::from(self.center_z),
            (self.scale + 1).clamp(0, 4),
            self.tracking_position,
            self.unlimited_tracking,
            &self.dimension,
        )
    }

    fn from_nbt(nbt: &NbtCompound) -> Option<Self> {
        let data = nbt.get_compound("data")?;
        let mut map = Self::new(
            data.get_int("xCenter")?,
            data.get_int("zCenter")?,
            data.get_byte("scale").unwrap_or(0),
            data.get_bool("trackingPosition").unwrap_or(true),
            data.get_bool("unlimitedTracking").unwrap_or(false),
            data.get_bool("locked").unwrap_or(false),
            data.get_string("dimension")
                .unwrap_or("minecraft:overworld")
                .to_string(),
        );
        if let Some(colors) = data.get_byte_array("colors")
            && colors.len() == MAP_SIZE * MAP_SIZE
        {
            for (to, from) in map.colors.iter_mut().zip(colors.iter()) {
                *to = *from as u8;
            }
        }
        for banner in data
            .get_list("banners")
            .into_iter()
            .flatten()
            .filter_map(NbtTag::extract_compound)
            .filter_map(MapBanner::from_nbt)
        {
            let id = banner.id();
            let (x, z) = (f64::from(banner.pos.0.x), f64::from(banner.pos.0.z));
            let (kind, name) = (
                banner.decoration(),
                banner.name.clone().map(TextComponent::text),
            );
            map.banners.insert(id.clone(), banner);
            map.add_decoration(kind, None, id, x, z, 180.0, name);
        }
        for frame in data
            .get_list("frames")
            .into_iter()
            .flatten()
            .filter_map(NbtTag::extract_compound)
            .filter_map(MapFrame::from_nbt)
        {
            map.frames.insert(frame_id(&frame.pos), frame);
            map.add_decoration(
                &MapDecorationType::FRAME,
                None,
                frame_key(frame.entity_id),
                f64::from(frame.pos.0.x),
                f64::from(frame.pos.0.z),
                f64::from(frame.rotation),
                None,
            );
        }
        map.dirty = false;
        Some(map)
    }

    fn to_nbt(&self) -> NbtCompound {
        let mut data = NbtCompound::new();
        data.put_string("dimension", self.dimension.clone());
        data.put_int("xCenter", self.center_x);
        data.put_int("zCenter", self.center_z);
        data.put_byte("scale", self.scale);
        data.put(
            "colors",
            NbtTag::ByteArray(self.colors.iter().map(|&c| c as i8).collect()),
        );
        data.put_bool("trackingPosition", self.tracking_position);
        data.put_bool("unlimitedTracking", self.unlimited_tracking);
        data.put_bool("locked", self.locked);
        data.put(
            "banners",
            NbtTag::List(
                self.banners
                    .values()
                    .map(|b| NbtTag::Compound(b.to_nbt()))
                    .collect(),
            ),
        );
        data.put(
            "frames",
            NbtTag::List(
                self.frames
                    .values()
                    .map(|f| NbtTag::Compound(f.to_nbt()))
                    .collect(),
            ),
        );
        let mut root = NbtCompound::new();
        root.put_compound("data", data);
        root.put_int("DataVersion", DATA_VERSION);
        root
    }

    fn holding_player(&mut self, uuid: Uuid) -> &mut HoldingPlayer {
        let index = self
            .carried_by
            .iter()
            .position(|h| h.uuid == uuid)
            .unwrap_or_else(|| {
                self.carried_by.push(HoldingPlayer::new(uuid));
                self.carried_by.len() - 1
            });
        &mut self.carried_by[index]
    }

    /// Vanilla `tickCarriedBy`: player markers of everyone carrying this map, the frame marker,
    /// and the decorations stored on the stack (explorer map targets).
    pub fn tick_carried_by(
        &mut self,
        player: &Player,
        map_id: i32,
        stack_decorations: &[(String, StackDecoration)],
        frame: Option<FrameRef>,
    ) {
        let world = player.world();
        self.holding_player(player.gameprofile.id);
        if !player_has_map(player, map_id) {
            self.remove_decoration(&player.gameprofile.name);
        }

        let mut index = 0;
        while index < self.carried_by.len() {
            let uuid = self.carried_by[index].uuid;
            let holder = world
                .server
                .upgrade()
                .and_then(|server| server.get_player_by_uuid(uuid));
            match holder {
                Some(holder) if frame.is_some() || player_has_map(&holder, map_id) => {
                    let name = holder.gameprofile.name.clone();
                    if frame.is_none()
                        && holder.world().dimension.minecraft_name == self.dimension
                        && self.tracking_position
                    {
                        let pos = holder.position();
                        let game_time = holder.world().get_world_age();
                        self.add_decoration(
                            &MapDecorationType::PLAYER,
                            Some(game_time),
                            name.clone(),
                            pos.x,
                            pos.z,
                            f64::from(holder.living_entity.entity.yaw.load()),
                            None,
                        );
                    }
                    if holder.gameprofile.id != player.gameprofile.id
                        && has_map_invisibility_item_equipped(&holder)
                    {
                        self.remove_decoration(&name);
                    }
                    index += 1;
                }
                holder => {
                    // Gone, or no longer carrying this map.
                    self.carried_by.remove(index);
                    if let Some(holder) = holder {
                        self.remove_decoration(&holder.gameprofile.name);
                    }
                }
            }
        }

        if let Some(frame) = frame
            && self.tracking_position
        {
            let marker = MapFrame {
                pos: frame.pos,
                rotation: frame.direction_2d * 90,
                entity_id: frame.entity_id,
            };
            let id = frame_id(&frame.pos);
            if let Some(old) = self.frames.get(&id).copied()
                && old.entity_id != frame.entity_id
            {
                self.remove_decoration(&frame_key(old.entity_id));
            }
            self.add_decoration(
                &MapDecorationType::FRAME,
                Some(world.get_world_age()),
                frame_key(frame.entity_id),
                f64::from(frame.pos.0.x),
                f64::from(frame.pos.0.z),
                f64::from(frame.direction_2d * 90),
                None,
            );
            if self.frames.insert(id, marker) != Some(marker) {
                self.dirty = true;
            }
        }

        if !stack_decorations
            .iter()
            .all(|(key, _)| self.decorations.contains_key(key))
        {
            for (key, entry) in stack_decorations {
                if !self.decorations.contains_key(key)
                    && let Some(kind) = MapDecorationType::from_name(
                        entry.kind.strip_prefix("minecraft:").unwrap_or(&entry.kind),
                    )
                {
                    self.add_decoration(
                        kind,
                        Some(world.get_world_age()),
                        key.clone(),
                        entry.x,
                        entry.z,
                        f64::from(entry.rotation),
                        None,
                    );
                }
            }
        }
    }

    fn remove_decoration(&mut self, id: &str) {
        if let Some(old) = self.decorations.shift_remove(id) {
            if old.kind.track_count {
                self.tracked_decoration_count -= 1;
            }
            self.set_decorations_dirty();
        }
    }

    /// Vanilla `addDecoration`; `game_time` is the level's, `None` for vanilla's null level.
    #[expect(clippy::too_many_arguments)]
    fn add_decoration(
        &mut self,
        kind: &'static MapDecorationType,
        game_time: Option<i64>,
        id: String,
        x: f64,
        z: f64,
        rotation: f64,
        name: Option<TextComponent>,
    ) {
        let scale = (1 << self.scale) as f32;
        let f = (x - f64::from(self.center_x)) as f32 / scale;
        let g = (z - f64::from(self.center_z)) as f32 / scale;
        let Some((kind, map_x, map_y, rot)) =
            self.decoration_location_and_type(kind, game_time, rotation, f, g)
        else {
            self.remove_decoration(&id);
            return;
        };
        let decoration = Decoration {
            kind,
            x: map_x,
            y: map_y,
            rot,
            name,
        };
        let new_counts = decoration.kind.track_count;
        let old = self.decorations.insert(id, decoration.clone());
        if old.as_ref() != Some(&decoration) {
            if old.is_some_and(|old| old.kind.track_count) {
                self.tracked_decoration_count -= 1;
            }
            if new_counts {
                self.tracked_decoration_count += 1;
            }
            self.set_decorations_dirty();
        }
    }

    /// Vanilla `calculateDecorationLocationAndType`.
    fn decoration_location_and_type(
        &self,
        kind: &'static MapDecorationType,
        game_time: Option<i64>,
        rotation: f64,
        f: f32,
        g: f32,
    ) -> Option<(&'static MapDecorationType, i8, i8, i8)> {
        let x = clamp_map_coordinate(f);
        let y = clamp_map_coordinate(g);
        if kind.id == MapDecorationType::PLAYER.id {
            // `playerDecorationTypeAndRotation`
            let kind = if is_inside_map(f, g) {
                kind
            } else if f.abs() < 320.0 && g.abs() < 320.0 {
                &MapDecorationType::PLAYER_OFF_MAP
            } else if self.unlimited_tracking {
                &MapDecorationType::PLAYER_OFF_LIMITS
            } else {
                return None;
            };
            return Some((kind, x, y, self.calculate_rotation(game_time, rotation)));
        }
        (is_inside_map(f, g) || self.unlimited_tracking)
            .then(|| (kind, x, y, self.calculate_rotation(game_time, rotation)))
    }

    /// Vanilla `calculateRotation`: spinning in the Nether.
    fn calculate_rotation(&self, game_time: Option<i64>, rotation: f64) -> i8 {
        if self.dimension == Dimension::THE_NETHER.minecraft_name
            && let Some(game_time) = game_time
        {
            let l = (game_time / 10) as i32;
            return ((l
                .wrapping_mul(l)
                .wrapping_mul(34_187_121)
                .wrapping_add(l.wrapping_mul(121)))
                >> 15
                & 15) as i8;
        }
        let rotation = if rotation < 0.0 {
            rotation - 8.0
        } else {
            rotation + 8.0
        };
        (rotation * 16.0 / 360.0) as i32 as i8
    }

    fn set_decorations_dirty(&mut self) {
        for holder in &mut self.carried_by {
            holder.dirty_decorations = true;
        }
    }

    fn set_colors_dirty(&mut self, x: usize, z: usize) {
        self.dirty = true;
        for holder in &mut self.carried_by {
            holder.mark_colors_dirty(x, z);
        }
    }

    /// Vanilla `updateColor`.
    pub fn update_color(&mut self, x: usize, z: usize, color: u8) -> bool {
        if self.colors[x + z * MAP_SIZE] == color {
            return false;
        }
        self.set_color(x, z, color);
        true
    }

    pub fn set_color(&mut self, x: usize, z: usize, color: u8) {
        self.colors[x + z * MAP_SIZE] = color;
        self.set_colors_dirty(x, z);
    }

    /// Vanilla `toggleBanner`: a filled map used on a banner adds or removes its marker.
    pub fn toggle_banner(&mut self, world: &World, pos: &BlockPos) -> bool {
        let x = f64::from(pos.0.x) + 0.5;
        let z = f64::from(pos.0.z) + 0.5;
        let scale = f64::from(1 << self.scale);
        let f = (x - f64::from(self.center_x)) / scale;
        let g = (z - f64::from(self.center_z)) / scale;
        if !(-63.0..=63.0).contains(&f) || !(-63.0..=63.0).contains(&g) {
            return false;
        }
        let Some(banner) = MapBanner::from_world(world, pos) else {
            return false;
        };
        let id = banner.id();
        if self.banners.get(&id) == Some(&banner) {
            self.banners.shift_remove(&id);
            self.remove_decoration(&id);
            self.dirty = true;
            return true;
        }
        if self.tracked_decoration_count > 256 {
            return false;
        }
        let (kind, name) = (banner.decoration(), banner.name.clone());
        self.banners.insert(id.clone(), banner);
        self.add_decoration(
            kind,
            Some(world.get_world_age()),
            id,
            x,
            z,
            180.0,
            name.map(TextComponent::text),
        );
        self.dirty = true;
        true
    }

    /// Vanilla `checkBanners`: markers of banners that changed or went away at `x`, `z`.
    fn check_banners(&mut self, world: &World, x: i32, z: i32) {
        let gone: Vec<String> = self
            .banners
            .iter()
            .filter(|(_, b)| b.pos.0.x == x && b.pos.0.z == z)
            .filter(|(_, b)| MapBanner::from_world(world, &b.pos).as_ref() != Some(*b))
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            self.banners.shift_remove(&id);
            self.remove_decoration(&id);
            self.dirty = true;
        }
    }

    /// Vanilla `removedFromFrame`.
    pub fn removed_from_frame(&mut self, pos: &BlockPos, entity_id: i32) {
        self.remove_decoration(&frame_key(entity_id));
        self.frames.shift_remove(&frame_id(pos));
        self.dirty = true;
    }

    /// Vanilla `getUpdatePacket` / `HoldingPlayer.nextUpdatePacket`, sent to `player`.
    pub fn send_update(&mut self, map_id: i32, player: &Player) {
        let Some(index) = self
            .carried_by
            .iter()
            .position(|h| h.uuid == player.gameprofile.id)
        else {
            return;
        };
        let holder = &mut self.carried_by[index];
        let patch = holder.dirty_data.then(|| {
            holder.dirty_data = false;
            let (x, y) = (holder.min_dirty_x, holder.min_dirty_y);
            let width = holder.max_dirty_x + 1 - x;
            let height = holder.max_dirty_y + 1 - y;
            let mut data = Vec::with_capacity(width * height);
            for row in 0..height {
                for column in 0..width {
                    data.push(self.colors[x + column + (y + row) * MAP_SIZE]);
                }
            }
            (x, y, width, height, data)
        });
        let send_decorations = holder.dirty_decorations && {
            let due = holder.tick.is_multiple_of(5);
            holder.tick = holder.tick.wrapping_add(1);
            due
        };
        if send_decorations {
            holder.dirty_decorations = false;
        }
        if patch.is_none() && !send_decorations {
            return;
        }
        let icons: Option<Vec<MapIcon>> = send_decorations.then(|| {
            self.decorations
                .values()
                .map(|d| MapIcon {
                    icon_type: VarInt(d.kind.id as i32),
                    x: d.x,
                    z: d.y,
                    direction: d.rot,
                    display_name: d.name.clone(),
                })
                .collect()
        });
        player.try_send_client_packet(&CMapItemData {
            map_id: VarInt(map_id),
            scale: self.scale,
            tracking_position: self.tracking_position,
            locked: self.locked,
            icons: icons.as_deref(),
            data: patch.as_ref().map(|(x, y, width, height, data)| MapPatch {
                columns: *width as u8,
                rows: *height as u8,
                x: *x as i8,
                z: *y as i8,
                data,
            }),
        });
    }

    /// Vanilla `MapItem.renderBiomePreviewMap`: the land/water outline explorer maps start with,
    /// from the biome source so far-away targets need no chunks.
    pub fn render_biome_preview(&mut self, world: &World) {
        use pumpkin_data::tag::{self, Taggable};
        if world.dimension.minecraft_name != self.dimension {
            return;
        }
        let scale = 1i32 << self.scale;
        let origin_x = self.center_x / scale - 64;
        let origin_z = self.center_z / scale - 64;
        let positions: Vec<(i32, i32, i32)> = (0..128)
            .flat_map(|m| {
                (0..128).map(move |n| {
                    (
                        (origin_x + n) * scale,
                        world.sea_level,
                        (origin_z + m) * scale,
                    )
                })
            })
            .collect();
        let biomes = world
            .level
            .world_gen()
            .noise_biomes_at(&positions)
            .unwrap_or_else(|| {
                positions
                    .iter()
                    .map(|&(x, y, z)| world.get_biome(&BlockPos::new(x, y, z)))
                    .collect()
            });
        // `bls[m * 128 + n]`, read back as `isBiomeWatery(bls, x, z) = bls[z * 128 + x]`.
        let watery: Vec<bool> = biomes
            .iter()
            .map(|biome| biome.has_tag(&tag::WorldgenBiome::MINECRAFT_WATER_ON_MAP_OUTLINES))
            .collect();
        let is_watery = |x: i32, z: i32| watery[(z * 128 + x) as usize];
        for m in 1..127 {
            for n in 1..127 {
                let mut neighbours = 0;
                for p in -1..2 {
                    for q in -1..2 {
                        if (p != 0 || q != 0) && is_watery(m + p, n + q) {
                            neighbours += 1;
                        }
                    }
                }
                // LOW 0, NORMAL 1, HIGH 2, LOWEST 3
                let mut brightness = 3;
                let mut color = MapColor::NONE_0.id;
                if is_watery(m, n) {
                    color = MapColor::COLOR_ORANGE.id;
                    if neighbours > 7 && n % 2 == 0 {
                        let wave = pumpkin_util::math::sin(n as f32) * 7.0;
                        match (m + wave as i32) / 8 % 5 {
                            0 | 4 => brightness = 0,
                            1 | 3 => brightness = 1,
                            2 => brightness = 2,
                            _ => {}
                        }
                    } else if neighbours > 7 {
                        color = MapColor::NONE_0.id;
                    } else if neighbours > 5 {
                        brightness = 1;
                    } else if neighbours > 1 {
                        brightness = 0;
                    }
                } else if neighbours > 0 {
                    color = MapColor::COLOR_BROWN.id;
                    brightness = if neighbours > 3 { 1 } else { 3 };
                }
                if color != MapColor::NONE_0.id {
                    self.set_color(m as usize, n as usize, (color << 2) | brightness);
                }
            }
        }
    }

    /// Vanilla `MapItem.update`: explores the area around `player`, a slice of columns per tick.
    #[expect(clippy::too_many_lines)]
    pub fn update(&mut self, world: &World, player: &Player) {
        if world.dimension.minecraft_name != self.dimension {
            return;
        }
        let scale = 1i32 << self.scale;
        let center_x = self.center_x;
        let center_z = self.center_z;
        let pos = player.position();
        let player_x = (pos.x - f64::from(center_x)).floor() as i32 / scale + 64;
        let player_z = (pos.z - f64::from(center_z)).floor() as i32 / scale + 64;
        let mut radius = 128 / scale;
        let has_ceiling = world.dimension.has_ceiling;
        if has_ceiling {
            radius /= 2;
        }
        let holder = self.holding_player(player.gameprofile.id);
        holder.step = holder.step.wrapping_add(1);
        let step = holder.step;
        let min_y = world.dimension.min_y;

        let mut changed = false;
        for o in (player_x - radius + 1)..(player_x + radius) {
            if (o & 15) as u32 != (step & 15) && !changed {
                continue;
            }
            changed = false;
            let mut previous_height = 0.0f64;
            for p in (player_z - radius - 1)..(player_z + radius) {
                if o < 0 || p < -1 || o >= 128 || p >= 128 {
                    continue;
                }
                let distance_sq = (o - player_x) * (o - player_x) + (p - player_z) * (p - player_z);
                let outer_ring = distance_sq > (radius - 2) * (radius - 2);
                let world_x = (center_x / scale + o - 64) * scale;
                let world_z = (center_z / scale + p - 64) * scale;
                let chunk_pos = Vector2::new(world_x >> 4, world_z >> 4);

                let mut colors: Vec<(u8, u32)> = Vec::new();
                let add_color = |colors: &mut Vec<(u8, u32)>, color: u8, count: u32| {
                    if let Some(entry) = colors.iter_mut().find(|(c, _)| *c == color) {
                        entry.1 += count;
                    } else {
                        colors.push((color, count));
                    }
                };
                let mut water_depth = 0;
                let mut height = if has_ceiling { 100.0 } else { 0.0f64 };
                if has_ceiling {
                    let mut seed = world_x.wrapping_add(world_z.wrapping_mul(231_871));
                    seed = seed
                        .wrapping_mul(seed)
                        .wrapping_mul(31_287_121)
                        .wrapping_add(seed.wrapping_mul(11));
                    if (seed >> 20) & 1 == 0 {
                        add_color(&mut colors, Block::DIRT.map_color, 10);
                    } else {
                        add_color(&mut colors, Block::STONE.map_color, 100);
                    }
                } else {
                    let read_columns = |chunk: &pumpkin_world::level::SyncChunk| {
                        let heightmap = chunk
                            .heightmap
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let mut out = Vec::with_capacity((scale * scale) as usize);
                        for u in 0..scale {
                            for v in 0..scale {
                                let (x, z) = (world_x + u, world_z + v);
                                let (local_x, local_z) = ((x & 15) as usize, (z & 15) as usize);
                                let state_at = |y: i32| {
                                    chunk
                                        .section
                                        .get_block_absolute_y(local_x, y, local_z)
                                        .unwrap_or(BlockStateId::AIR)
                                };
                                let mut y = heightmap.get(
                                    pumpkin_world::chunk::ChunkHeightmapType::WorldSurface,
                                    x,
                                    z,
                                    min_y,
                                ) + 1;
                                let mut depth = 0;
                                let state = if y <= min_y {
                                    Block::BEDROCK.default_state.id
                                } else {
                                    let mut state;
                                    loop {
                                        y -= 1;
                                        state = state_at(y);
                                        if map_color(state) != MapColor::NONE_0.id || y <= min_y {
                                            break;
                                        }
                                    }
                                    if y > min_y && fluid_block(state).is_some() {
                                        let mut below = y - 1;
                                        loop {
                                            let fluid_state = state_at(below);
                                            below -= 1;
                                            depth += 1;
                                            if below <= min_y || fluid_block(fluid_state).is_none()
                                            {
                                                break;
                                            }
                                        }
                                        correct_state_for_fluid_block(state)
                                    } else {
                                        state
                                    }
                                };
                                out.push((x, z, y, depth, state));
                            }
                        }
                        out
                    };
                    // Vanilla `Level.getChunk` loads (or generates) the chunk right here,
                    // holding up the tick like vanilla does.
                    let loaded = world.level.read_chunk_sync(&chunk_pos, read_columns);
                    let columns = if let Some(columns) = loaded {
                        columns
                    } else {
                        let Some(server) = world.server.upgrade() else {
                            continue;
                        };
                        server
                            .runtime
                            .block_on(world.level.get_or_fetch_chunk(chunk_pos, read_columns))
                    };
                    for (x, z, y, depth, state) in columns {
                        self.check_banners(world, x, z);
                        water_depth += depth;
                        height += f64::from(y) / f64::from(scale * scale);
                        add_color(&mut colors, map_color(state), 1);
                    }
                }
                water_depth /= scale * scale;
                // `Multisets.copyHighestCountFirst`: most common, first seen on ties.
                let mut color = MapColor::NONE_0.id;
                let mut best = 0;
                for (c, count) in &colors {
                    if *count > best {
                        best = *count;
                        color = *c;
                    }
                }
                let checker = f64::from((o + p) & 1);
                // HIGH 2, LOW 0, else NORMAL 1
                let brightness = |high: bool, low: bool| match (high, low) {
                    (true, _) => 2,
                    (_, true) => 0,
                    _ => 1,
                };
                let brightness = if color == MapColor::WATER.id {
                    let f = f64::from(water_depth) * 0.1 + checker * 0.2;
                    brightness(f < 0.5, f > 0.9)
                } else {
                    let f = (height - previous_height) * 4.0 / f64::from(scale + 4)
                        + (checker - 0.5) * 0.4;
                    brightness(f > 0.6, f < -0.6)
                };
                previous_height = height;
                if p >= 0 && distance_sq < radius * radius && (!outer_ring || (o + p) & 1 != 0) {
                    changed |= self.update_color(o as usize, p as usize, (color << 2) | brightness);
                }
            }
        }
    }
}

/// Decorations stored on the map item (`map_decorations`), e.g. explorer map targets.
#[derive(Clone, Debug, PartialEq)]
pub struct StackDecoration {
    pub kind: String,
    pub x: f64,
    pub z: f64,
    pub rotation: f32,
}

/// The `map_decorations` entries of a map stack.
#[must_use]
pub fn stack_decorations(
    stack: &pumpkin_data::item_stack::ItemStack,
) -> Vec<(String, StackDecoration)> {
    use pumpkin_data::data_component_impl::MapDecorationsImpl;
    stack
        .get_data_component::<MapDecorationsImpl>()
        .map(|d| {
            d.decorations
                .iter()
                .map(|(key, e)| {
                    (
                        key.clone(),
                        StackDecoration {
                            kind: e.kind.clone(),
                            x: e.x,
                            z: e.z,
                            rotation: e.rotation,
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn is_inside_map(f: f32, g: f32) -> bool {
    (-63.0..=63.0).contains(&f) && (-63.0..=63.0).contains(&g)
}

fn clamp_map_coordinate(f: f32) -> i8 {
    if f <= -63.0 {
        -128
    } else if f >= 63.0 {
        127
    } else {
        (f64::from(f * 2.0) + 0.5) as i32 as i8
    }
}

/// Vanilla `hasMapInvisibilityItemEquipped`: armor in `map_invisibility_equipment` (carved
/// pumpkins, mob heads) hides other players from the map.
fn has_map_invisibility_item_equipped(player: &Player) -> bool {
    use pumpkin_data::tag::{self, Taggable};
    let equipment = player
        .living_entity
        .entity_equipment
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    equipment.equipment.iter().any(|(slot, stack)| {
        !matches!(
            slot,
            pumpkin_data::data_component_impl::EquipmentSlot::MainHand(_)
                | pumpkin_data::data_component_impl::EquipmentSlot::OffHand(_)
        ) && stack
            .item
            .has_tag(&tag::Item::MINECRAFT_MAP_INVISIBILITY_EQUIPMENT)
    })
}

/// Whether the player's inventory (main, armor, off hand) holds a map with this id.
fn player_has_map(player: &Player, map_id: i32) -> bool {
    use pumpkin_data::data_component_impl::MapIdImpl;
    let is_map = |stack: &pumpkin_data::item_stack::ItemStack| {
        stack
            .get_data_component::<MapIdImpl>()
            .is_some_and(|id| id.id == map_id)
    };
    let in_main = player
        .inventory
        .main_inventory
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(is_map);
    in_main
        || player
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .equipment
            .values()
            .any(is_map)
}

/// The map color of a block state; vanilla colors a few blocks per state.
#[must_use]
pub fn map_color(state: BlockStateId) -> u8 {
    let block = Block::from_state_id(state);
    let base = block.map_color;
    let prop = |key: &str| -> Option<String> {
        block.properties(state).and_then(|props| {
            props
                .to_props()
                .into_iter()
                .find(|p| p.key_str() == key)
                .map(|p| p.value_str().to_string())
        })
    };
    // `logProperties(top, side)`: the default (axis=y) color is the top one.
    let side = match block.name {
        "oak_log" | "jungle_log" | "mangrove_log" | "poplar_log" => Some(MapColor::PODZOL.id),
        "spruce_log" | "dark_oak_log" => Some(MapColor::COLOR_BROWN.id),
        "birch_log" => Some(MapColor::QUARTZ.id),
        "acacia_log" => Some(MapColor::STONE.id),
        "cherry_log" => Some(MapColor::TERRACOTTA_GRAY.id),
        "stripped_cherry_log" => Some(MapColor::TERRACOTTA_PINK.id),
        "pale_oak_log" => Some(Block::PALE_OAK_WOOD.map_color),
        "bamboo_block" => Some(MapColor::PLANT.id),
        _ => None,
    };
    if let Some(side) = side {
        return if prop("axis").as_deref() == Some("y") {
            base
        } else {
            side
        };
    }
    match block.name {
        "wheat" => {
            if prop("age").and_then(|a| a.parse::<i32>().ok()).unwrap_or(0) >= 6 {
                MapColor::COLOR_YELLOW.id
            } else {
                MapColor::PLANT.id
            }
        }
        "barrier" | "light" if prop("waterlogged").as_deref() == Some("true") => MapColor::WATER.id,
        name if name.ends_with("_bed") && prop("part").as_deref() == Some("head") => {
            MapColor::WOOL.id
        }
        _ => base,
    }
}

/// The fluid block of a state that holds water or lava, `None` otherwise.
fn fluid_block(state: BlockStateId) -> Option<&'static Block> {
    let fluid = pumpkin_data::fluid::Fluid::of_block_state(state);
    if fluid.matches_type(&pumpkin_data::fluid::Fluid::WATER) {
        Some(&Block::WATER)
    } else if fluid.matches_type(&pumpkin_data::fluid::Fluid::LAVA) {
        Some(&Block::LAVA)
    } else {
        None
    }
}

/// Vanilla `getCorrectStateForFluidBlock`: the fluid shows unless the top face is sturdy.
fn correct_state_for_fluid_block(state: BlockStateId) -> BlockStateId {
    match fluid_block(state) {
        Some(fluid) if !state.to_state().is_side_solid(BlockDirection::Up) => {
            fluid.default_state.id
        }
        _ => state,
    }
}

/// Vanilla map saved data on the server: `data/minecraft/maps/<id>.dat` and `last_id.dat`.
pub struct MapManager {
    maps: DashMap<i32, Arc<Mutex<MapData>>>,
    folder: PathBuf,
    last_id: AtomicI32,
    last_id_dirty: AtomicBool,
}

impl MapManager {
    /// `world_folder` is the world root; `level_next_id` is the counter older Pumpkin worlds
    /// kept in `level.dat` (the next id to hand out).
    #[must_use]
    pub fn new(world_folder: &Path, level_next_id: i32) -> Self {
        let folder = world_folder.join("data").join("minecraft").join("maps");
        let stored = std::fs::File::open(folder.join("last_id.dat"))
            .ok()
            .and_then(|file| read_gzip_compound_tag(std::io::BufReader::new(file)).ok())
            .and_then(|nbt| nbt.get_compound("data").and_then(|d| d.get_int("map")))
            .unwrap_or(-1);
        Self {
            maps: DashMap::new(),
            folder,
            last_id: AtomicI32::new(stored.max(level_next_id - 1)),
            last_id_dirty: AtomicBool::new(false),
        }
    }

    /// Vanilla `MapItem.getSavedData`: loaded on first use, `None` for unknown ids.
    #[must_use]
    pub fn get_map(&self, id: i32) -> Option<Arc<Mutex<MapData>>> {
        if let Some(map) = self.maps.get(&id) {
            return Some(map.clone());
        }
        let file = std::fs::File::open(self.folder.join(format!("{id}.dat"))).ok()?;
        let nbt = match read_gzip_compound_tag(std::io::BufReader::new(file)) {
            Ok(nbt) => nbt,
            Err(err) => {
                warn!("Failed to read map {id}: {err}");
                return None;
            }
        };
        let map = Arc::new(Mutex::new(MapData::from_nbt(&nbt)?));
        Some(self.maps.entry(id).or_insert(map).clone())
    }

    /// Vanilla `getFreeMapId`.
    pub fn next_id(&self) -> i32 {
        self.last_id_dirty.store(true, Ordering::Relaxed);
        self.last_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// The id the next new map gets (kept in `level.dat` for older Pumpkin versions).
    #[must_use]
    pub fn peek_next_id(&self) -> i32 {
        self.last_id.load(Ordering::SeqCst) + 1
    }

    /// Stores `data` under a new id.
    pub fn add(&self, data: MapData) -> i32 {
        let id = self.next_id();
        self.maps.insert(id, Arc::new(Mutex::new(data)));
        id
    }

    /// Vanilla `MapItem.create`: a fresh map around `x`, `z`.
    pub fn create_map(
        &self,
        x: f64,
        z: f64,
        scale: i8,
        tracking_position: bool,
        unlimited_tracking: bool,
        dimension: &str,
    ) -> i32 {
        self.add(MapData::create_fresh(
            x,
            z,
            scale,
            tracking_position,
            unlimited_tracking,
            dimension,
        ))
    }

    /// Writes changed maps and the id counter.
    pub fn save_all(&self) {
        let dirty: Vec<(i32, NbtCompound)> = self
            .maps
            .iter()
            .filter_map(|entry| {
                let mut map = entry
                    .value()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                map.dirty.then(|| {
                    map.dirty = false;
                    (*entry.key(), map.to_nbt())
                })
            })
            .collect();
        let write_id = self.last_id_dirty.swap(false, Ordering::Relaxed);
        if dirty.is_empty() && !write_id {
            return;
        }
        if let Err(err) = std::fs::create_dir_all(&self.folder) {
            warn!("Failed to create {}: {err}", self.folder.display());
            return;
        }
        for (id, nbt) in dirty {
            Self::write(&self.folder.join(format!("{id}.dat")), nbt);
        }
        if write_id {
            let mut data = NbtCompound::new();
            data.put_int("map", self.last_id.load(Ordering::SeqCst));
            let mut root = NbtCompound::new();
            root.put_compound("data", data);
            root.put_int("DataVersion", DATA_VERSION);
            Self::write(&self.folder.join("last_id.dat"), root);
        }
    }

    /// Writes to a temporary file first so a crash never leaves a torn map.
    fn write(path: &Path, nbt: NbtCompound) {
        let tmp = path.with_extension("dat_tmp");
        let result = std::fs::File::create(&tmp)
            .map_err(|e| e.to_string())
            .and_then(|file| {
                write_gzip_compound_tag(nbt, std::io::BufWriter::new(file))
                    .map_err(|e| e.to_string())
            })
            .and_then(|()| std::fs::rename(&tmp, path).map_err(|e| e.to_string()));
        if let Err(err) = result {
            warn!("Failed to save {}: {err}", path.display());
        }
    }
}

/// Vanilla `MapItem.onCraftedPostProcess` for stacks made outside a cartography table (the
/// crafter); `true` if the stack carried a pending lock/scale.
pub fn resolve_map_post_processing(
    stack: &mut pumpkin_data::item_stack::ItemStack,
    server: &crate::server::Server,
) -> bool {
    use pumpkin_data::data_component_impl::MapPostProcessingImpl;
    let pending = stack
        .get_data_component::<MapPostProcessingImpl>()
        .is_some_and(|p| p.processing.is_some());
    if pending {
        crate::entity::player::map_post_process(server, stack);
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_fresh_snaps_to_the_map_grid() {
        let map = MapData::create_fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        assert_eq!((map.center_x, map.center_z), (0, 0));
        let map = MapData::create_fresh(100.0, -100.0, 0, true, false, "minecraft:overworld");
        assert_eq!((map.center_x, map.center_z), (128, -128));
        let map = MapData::create_fresh(1000.0, 0.0, 2, true, false, "minecraft:overworld");
        assert_eq!(map.center_x, 1216);
        assert_eq!(map.scaled().scale, 3);
    }

    #[test]
    fn map_coordinates_clamp_like_vanilla() {
        assert_eq!(clamp_map_coordinate(10.3), 21);
        assert_eq!(clamp_map_coordinate(-10.3), -20);
        assert_eq!(clamp_map_coordinate(-63.0), -128);
        assert_eq!(clamp_map_coordinate(63.0), 127);
    }

    #[test]
    fn player_markers_rotate_and_leave_the_map() {
        let mut map = MapData::create_fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        assert_eq!(map.calculate_rotation(Some(0), 90.0), 4);
        assert_eq!(map.calculate_rotation(Some(0), -90.0), -4);

        let player = &MapDecorationType::PLAYER;
        map.add_decoration(player, Some(0), "a".into(), 20.0, 0.0, 0.0, None);
        assert_eq!(map.decorations["a"].kind.id, MapDecorationType::PLAYER.id);
        assert_eq!(map.decorations["a"].x, 40);
        map.add_decoration(player, Some(0), "a".into(), 200.0, 0.0, 0.0, None);
        assert_eq!(
            map.decorations["a"].kind.id,
            MapDecorationType::PLAYER_OFF_MAP.id
        );
        assert_eq!(map.decorations["a"].x, 127);
        map.add_decoration(player, Some(0), "a".into(), 1000.0, 0.0, 0.0, None);
        assert!(!map.decorations.contains_key("a"));
        map.unlimited_tracking = true;
        map.add_decoration(player, Some(0), "a".into(), 1000.0, 0.0, 0.0, None);
        assert_eq!(
            map.decorations["a"].kind.id,
            MapDecorationType::PLAYER_OFF_LIMITS.id
        );
    }

    #[test]
    fn saved_data_round_trips() {
        let mut map = MapData::create_fresh(300.0, 40.0, 1, true, true, "minecraft:the_nether");
        map.set_color(3, 5, 77);
        map.banners.insert(
            "banner-1,2,3".into(),
            MapBanner {
                pos: BlockPos::new(1, 2, 3),
                color: "red".into(),
                name: Some("Home".into()),
            },
        );
        let loaded = MapData::from_nbt(&map.to_nbt()).expect("map should load");
        assert_eq!(
            (loaded.center_x, loaded.center_z),
            (map.center_x, map.center_z)
        );
        assert_eq!(loaded.scale, 1);
        assert!(loaded.unlimited_tracking);
        assert_eq!(loaded.dimension, "minecraft:the_nether");
        assert_eq!(loaded.colors[3 + 5 * MAP_SIZE], 77);
        assert_eq!(loaded.banners.len(), 1);
        // The banner marker comes back with the data.
        assert_eq!(
            loaded.decorations["banner-1,2,3"].kind.id,
            MapDecorationType::BANNER_RED.id
        );
    }

    #[test]
    fn holders_get_only_the_changed_rectangle() {
        let mut map = MapData::create_fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        let uuid = Uuid::from_u128(1);
        let holder = map.holding_player(uuid);
        // A new holder gets everything once.
        assert!(holder.dirty_data);
        assert_eq!((holder.max_dirty_x, holder.max_dirty_y), (127, 127));
        map.holding_player(uuid).dirty_data = false;
        map.update_color(10, 20, 5);
        map.update_color(12, 21, 5);
        assert!(!map.update_color(12, 21, 5));
        let holder = map.holding_player(uuid);
        assert_eq!(
            (
                holder.min_dirty_x,
                holder.min_dirty_y,
                holder.max_dirty_x,
                holder.max_dirty_y
            ),
            (10, 20, 12, 21)
        );
    }

    #[test]
    fn state_dependent_map_colors() {
        use pumpkin_data::block_properties::PaleOakWoodLikeProperties;
        let log = Block::OAK_LOG;
        let mut props = PaleOakWoodLikeProperties::default(&log);
        assert_eq!(map_color(props.to_state_id(&log)), MapColor::WOOD.id);
        props.axis = pumpkin_data::block_properties::Axis::X;
        assert_eq!(map_color(props.to_state_id(&log)), MapColor::PODZOL.id);
        assert_eq!(map_color(Block::WATER.default_state.id), MapColor::WATER.id);
    }
}
