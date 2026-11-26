//! Vanilla `SculkSpreader`, `SculkBehaviour`, and the charge handling of `SculkBlock` and
//! `SculkVeinBlock`: experience from deaths near a catalyst turns into creeping sculk.

use std::sync::Arc;

use pumpkin_data::block_properties::{GlowLichenLikeProperties, SculkShriekerLikeProperties};
use pumpkin_data::fluid::Fluid;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{Block, BlockDirection, BlockStateId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;
use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

use super::multiface_spreader::{
    DEFAULT_SPREAD_ORDER, MultifaceSpreader, SpreadType, can_attach_to, has_face, set_face,
};
use crate::world::World;

pub const MAX_CHARGE: i32 = 1000;
const MAX_CURSORS: usize = 32;
const SHRIEKER_PLACEMENT_RATE: i32 = 11;
const MAX_CURSOR_DISTANCE: i32 = 1024;
const MAX_GROWTH_RATE_RADIUS: i32 = 24;

static VEIN_SPREADER: MultifaceSpreader = MultifaceSpreader::sculk_vein(DEFAULT_SPREAD_ORDER);
static SAME_SPACE_SPREADER: MultifaceSpreader =
    MultifaceSpreader::sculk_vein(&[SpreadType::SamePosition]);

/// Vanilla `SculkSpreader.createLevelSpreader` settings.
pub struct SculkSpreader {
    growth_spawn_cost: i32,
    no_growth_radius: i32,
    charge_decay_rate: i32,
    additional_decay_rate: i32,
    cursors: Vec<ChargeCursor>,
}

impl Default for SculkSpreader {
    fn default() -> Self {
        Self {
            growth_spawn_cost: 10,
            no_growth_radius: 4,
            charge_decay_rate: 10,
            additional_decay_rate: 5,
            cursors: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct ChargeCursor {
    pos: BlockPos,
    charge: i32,
    update_delay: i32,
    decay_delay: i32,
    facings: Option<Vec<BlockDirection>>,
}

impl ChargeCursor {
    const fn new(pos: BlockPos, charge: i32) -> Self {
        Self {
            pos,
            charge,
            update_delay: 1,
            decay_delay: 0,
            facings: None,
        }
    }

    fn read_nbt(nbt: &NbtCompound) -> Option<Self> {
        let pos = nbt.get_int_array("pos")?;
        if pos.len() != 3 {
            return None;
        }
        let facings = nbt.get_list("facings").map(|list| {
            list.iter()
                .filter_map(|tag| tag.extract_string())
                .filter_map(direction_from_name)
                .collect()
        });
        Some(Self {
            pos: BlockPos::new(pos[0], pos[1], pos[2]),
            charge: nbt.get_int("charge")?.clamp(0, MAX_CHARGE),
            update_delay: nbt.get_int("update_delay").unwrap_or(1).max(0),
            decay_delay: nbt.get_int("decay_delay").unwrap_or(1).clamp(0, 2),
            facings,
        })
    }

    fn write_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put(
            "pos",
            NbtTag::IntArray(vec![self.pos.0.x, self.pos.0.y, self.pos.0.z]),
        );
        nbt.put_int("charge", self.charge);
        nbt.put_int("decay_delay", self.decay_delay);
        nbt.put_int("update_delay", self.update_delay);
        if let Some(facings) = &self.facings {
            nbt.put_list(
                "facings",
                facings
                    .iter()
                    .map(|direction| NbtTag::String(direction_name(*direction).into()))
                    .collect(),
            );
        }
        nbt
    }

    fn update(
        &mut self,
        world: &Arc<World>,
        origin: &BlockPos,
        rng: &mut impl Rng,
        spreader: &SculkSpreader,
    ) {
        if self.charge <= 0 {
            return;
        }
        if self.update_delay > 0 {
            self.update_delay -= 1;
            return;
        }
        let mut state = world.get_block_state_id(&self.pos);
        let mut behaviour = Behaviour::of(state);
        if behaviour.attempt_spread_vein(world, &self.pos, state, self.facings.as_deref()) {
            if behaviour.can_change_block_state_on_spread() {
                state = world.get_block_state_id(&self.pos);
                behaviour = Behaviour::of(state);
            }
            world.play_block_sound(Sound::BlockSculkSpread, SoundCategory::Blocks, self.pos);
        }
        self.charge = behaviour.attempt_use_charge(self, world, origin, rng, spreader);
        if self.charge <= 0 {
            behaviour.on_discharged(world, state, &self.pos);
            return;
        }
        if let Some(next) = valid_movement_pos(world, &self.pos, rng) {
            behaviour.on_discharged(world, state, &self.pos);
            self.pos = next;
            state = world.get_block_state_id(&next);
        }
        if Behaviour::is_sculk(state) {
            self.facings = Some(available_faces(state));
        }
        self.decay_delay = behaviour.update_decay_delay(self.decay_delay);
        self.update_delay = 1;
    }

    fn merge_with(&mut self, other: &mut Self) {
        self.charge += other.charge;
        other.charge = 0;
        self.update_delay = self.update_delay.min(other.update_delay);
    }
}

const fn direction_name(direction: BlockDirection) -> &'static str {
    match direction {
        BlockDirection::Down => "down",
        BlockDirection::Up => "up",
        BlockDirection::North => "north",
        BlockDirection::South => "south",
        BlockDirection::West => "west",
        BlockDirection::East => "east",
    }
}

fn direction_from_name(name: &str) -> Option<BlockDirection> {
    BlockDirection::all()
        .into_iter()
        .find(|direction| direction_name(*direction) == name)
}

/// Vanilla `MultifaceBlock.availableFaces`: empty for anything but a vein.
fn available_faces(state: BlockStateId) -> Vec<BlockDirection> {
    if state.to_block() != &Block::SCULK_VEIN {
        return Vec::new();
    }
    BlockDirection::all()
        .into_iter()
        .filter(|direction| has_face(state, *direction))
        .collect()
}

/// Vanilla `MultifaceBlock.pack`.
fn pack_faces(faces: &[BlockDirection]) -> i32 {
    faces
        .iter()
        .fold(0, |packed, direction| packed | (1 << direction.to_index()))
}

fn non_corner_neighbour_offsets() -> Vec<Vector3<i32>> {
    let mut offsets = Vec::with_capacity(18);
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                if (x == 0 || y == 0 || z == 0) && (x, y, z) != (0, 0, 0) {
                    offsets.push(Vector3::new(x, y, z));
                }
            }
        }
    }
    offsets
}

/// Vanilla `getValidMovementPos`: prefer a neighbouring vein that touches replaceable stone.
fn valid_movement_pos(world: &World, pos: &BlockPos, rng: &mut impl Rng) -> Option<BlockPos> {
    let mut offsets = non_corner_neighbour_offsets();
    offsets.shuffle(rng);
    let mut result = None;
    for offset in offsets {
        let next = pos.offset(offset);
        let state = world.get_block_state_id(&next);
        if Behaviour::is_sculk(state) && is_movement_unobstructed(world, pos, &next) {
            result = Some(next);
            if has_substrate_access(world, state, &next) {
                break;
            }
        }
    }
    result
}

fn is_movement_unobstructed(world: &World, from: &BlockPos, to: &BlockPos) -> bool {
    if from.manhattan_distance(*to) == 1 {
        return true;
    }
    let d = to.0 - from.0;
    let axis = |value: i32, negative: BlockDirection, positive: BlockDirection| {
        if value < 0 { negative } else { positive }
    };
    let dx = axis(d.x, BlockDirection::West, BlockDirection::East);
    let dy = axis(d.y, BlockDirection::Down, BlockDirection::Up);
    let dz = axis(d.z, BlockDirection::North, BlockDirection::South);
    let unobstructed = |direction: BlockDirection| {
        let neighbour = from.offset(direction.to_offset());
        !world.is_face_sturdy(&neighbour, direction.opposite())
    };
    if d.x == 0 {
        unobstructed(dy) || unobstructed(dz)
    } else if d.y == 0 {
        unobstructed(dx) || unobstructed(dz)
    } else {
        unobstructed(dx) || unobstructed(dy)
    }
}

/// Vanilla `SculkVeinBlock.hasSubstrateAccess`.
fn has_substrate_access(world: &World, state: BlockStateId, pos: &BlockPos) -> bool {
    state.to_block() == &Block::SCULK_VEIN
        && BlockDirection::all().into_iter().any(|direction| {
            has_face(state, direction)
                && world
                    .get_block(&pos.offset(direction.to_offset()))
                    .has_tag(&tag::Block::MINECRAFT_SCULK_REPLACEABLE)
        })
}

/// Vanilla `SculkVeinBlock.regrow`.
fn regrow(world: &Arc<World>, pos: &BlockPos, faces: &[BlockDirection]) -> bool {
    let mut props = GlowLichenLikeProperties::from_state_id(Block::SCULK_VEIN.default_state.id);
    let mut attached = false;
    for direction in faces {
        if can_attach_to(world, pos, *direction) {
            set_face(&mut props, *direction, true);
            attached = true;
        }
    }
    if !attached {
        return false;
    }
    props.waterlogged = !world.get_fluid_and_fluid_state(pos).1.is_empty;
    world.set_block_state(
        pos,
        props.to_state_id(&Block::SCULK_VEIN),
        BlockFlags::NOTIFY_ALL,
    );
    true
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Behaviour {
    Default,
    Sculk,
    Vein,
}

impl Behaviour {
    fn of(state: BlockStateId) -> Self {
        let block = state.to_block();
        if block == &Block::SCULK {
            Self::Sculk
        } else if block == &Block::SCULK_VEIN {
            Self::Vein
        } else {
            Self::Default
        }
    }

    fn is_sculk(state: BlockStateId) -> bool {
        Self::of(state) != Self::Default
    }

    const fn can_change_block_state_on_spread(self) -> bool {
        !matches!(self, Self::Sculk)
    }

    const fn update_decay_delay(self, delay: i32) -> i32 {
        match self {
            Self::Default => {
                if delay - 1 > 0 {
                    delay - 1
                } else {
                    0
                }
            }
            Self::Sculk | Self::Vein => 1,
        }
    }

    fn attempt_spread_vein(
        self,
        world: &Arc<World>,
        pos: &BlockPos,
        state: BlockStateId,
        facings: Option<&[BlockDirection]>,
    ) -> bool {
        match (self, facings) {
            (Self::Default, None) => {
                SAME_SPACE_SPREADER.spread_all(world, pos, world.get_block_state_id(pos)) > 0
            }
            (Self::Default, Some(faces)) if !faces.is_empty() => {
                let block_state = world.get_block_state(pos);
                (block_state.is_air()
                    || world
                        .get_fluid_and_fluid_state(pos)
                        .0
                        .matches_type(&Fluid::WATER))
                    && regrow(world, pos, faces)
            }
            _ => VEIN_SPREADER.spread_all(world, pos, state) > 0,
        }
    }

    fn attempt_use_charge(
        self,
        cursor: &ChargeCursor,
        world: &Arc<World>,
        origin: &BlockPos,
        rng: &mut impl Rng,
        spreader: &SculkSpreader,
    ) -> i32 {
        match self {
            Self::Default => {
                if cursor.decay_delay > 0 {
                    cursor.charge
                } else {
                    0
                }
            }
            Self::Sculk => sculk_use_charge(cursor, world, origin, rng, spreader),
            Self::Vein => {
                if attempt_place_sculk(world, &cursor.pos, rng) {
                    cursor.charge - 1
                } else if rng.random_range(0..spreader.charge_decay_rate) == 0 {
                    #[expect(clippy::cast_possible_truncation, reason = "vanilla floors a float")]
                    let halved = (cursor.charge as f32 * 0.5).floor() as i32;
                    halved
                } else {
                    cursor.charge
                }
            }
        }
    }

    /// Vanilla `SculkVeinBlock.onDischarged`: drop faces now covered by sculk.
    fn on_discharged(self, world: &Arc<World>, state: BlockStateId, pos: &BlockPos) {
        if self != Self::Vein || state.to_block() != &Block::SCULK_VEIN {
            return;
        }
        let mut props = GlowLichenLikeProperties::from_state_id(state);
        for direction in BlockDirection::all() {
            if has_face(state, direction)
                && world.get_block(&pos.offset(direction.to_offset())) == &Block::SCULK
            {
                set_face(&mut props, direction, false);
            }
        }
        let new_state = props.to_state_id(&Block::SCULK_VEIN);
        let any_face = BlockDirection::all()
            .into_iter()
            .any(|direction| has_face(new_state, direction));
        let new_state = if any_face {
            new_state
        } else if world.get_fluid_and_fluid_state(pos).1.is_empty {
            Block::AIR.default_state.id
        } else {
            Block::WATER.default_state.id
        };
        world.set_block_state(pos, new_state, BlockFlags::NOTIFY_ALL);
    }
}

/// Vanilla `SculkVeinBlock.attemptPlaceSculk`: turn the stone a vein clings to into sculk.
fn attempt_place_sculk(world: &Arc<World>, pos: &BlockPos, rng: &mut impl Rng) -> bool {
    let state = world.get_block_state_id(pos);
    let mut directions = BlockDirection::all();
    directions.shuffle(rng);
    for direction in directions {
        if !has_face(state, direction) {
            continue;
        }
        let target = pos.offset(direction.to_offset());
        if !world
            .get_block(&target)
            .has_tag(&tag::Block::MINECRAFT_SCULK_REPLACEABLE)
        {
            continue;
        }
        let sculk = Block::SCULK.default_state.id;
        world.set_block_state(&target, sculk, BlockFlags::NOTIFY_ALL);
        world.play_block_sound(Sound::BlockSculkSpread, SoundCategory::Blocks, target);
        VEIN_SPREADER.spread_all(world, &target, sculk);
        let opposite = direction.opposite();
        for other in BlockDirection::all() {
            if other == opposite {
                continue;
            }
            let neighbour = target.offset(other.to_offset());
            let neighbour_state = world.get_block_state_id(&neighbour);
            if neighbour_state.to_block() == &Block::SCULK_VEIN {
                Behaviour::Vein.on_discharged(world, neighbour_state, &neighbour);
            }
        }
        return true;
    }
    false
}

/// Vanilla `SculkBlock.attemptUseCharge`: grow sensors and shriekers, or decay the charge.
fn sculk_use_charge(
    cursor: &ChargeCursor,
    world: &Arc<World>,
    origin: &BlockPos,
    rng: &mut impl Rng,
    spreader: &SculkSpreader,
) -> i32 {
    let charge = cursor.charge;
    if charge == 0 || rng.random_range(0..spreader.charge_decay_rate) != 0 {
        return charge;
    }
    let pos = cursor.pos;
    let radius = spreader.no_growth_radius;
    let near_origin = pos.squared_distance(origin) < radius * radius;
    if !near_origin && can_place_growth(world, &pos) {
        let cost = spreader.growth_spawn_cost;
        if rng.random_range(0..cost) < charge {
            let above = pos.up();
            let growth = random_growth_state(world, &above, rng);
            world.set_block_state(&above, growth, BlockFlags::NOTIFY_ALL);
            let sound = if growth.to_block() == &Block::SCULK_SHRIEKER {
                Sound::BlockSculkShriekerPlace
            } else {
                Sound::BlockSculkSensorPlace
            };
            world.play_block_sound(sound, SoundCategory::Blocks, pos);
        }
        return (charge - cost).max(0);
    }
    if rng.random_range(0..spreader.additional_decay_rate) != 0 {
        return charge;
    }
    charge
        - if near_origin {
            1
        } else {
            decay_penalty(spreader, &pos, origin, charge)
        }
}

fn decay_penalty(spreader: &SculkSpreader, pos: &BlockPos, origin: &BlockPos, charge: i32) -> i32 {
    let radius = spreader.no_growth_radius;
    #[expect(clippy::cast_precision_loss, reason = "block distances are small")]
    let distance = (pos.squared_distance(origin) as f32).sqrt() - radius as f32;
    let max = (MAX_GROWTH_RATE_RADIUS - radius).pow(2);
    #[expect(clippy::cast_precision_loss, reason = "small radius")]
    let factor = (distance * distance / max as f32).min(1.0);
    #[expect(clippy::cast_possible_truncation, reason = "vanilla truncates")]
    #[expect(clippy::cast_precision_loss, reason = "charge is at most 1000")]
    let penalty = (charge as f32 * factor * 0.5) as i32;
    penalty.max(1)
}

fn random_growth_state(world: &World, pos: &BlockPos, rng: &mut impl Rng) -> BlockStateId {
    let waterlogged = !world.get_fluid_and_fluid_state(pos).1.is_empty;
    if rng.random_range(0..SHRIEKER_PLACEMENT_RATE) == 0 {
        let mut props =
            SculkShriekerLikeProperties::from_state_id(Block::SCULK_SHRIEKER.default_state.id);
        props.can_summon = false;
        props.waterlogged = waterlogged;
        props.to_state_id(&Block::SCULK_SHRIEKER)
    } else {
        let mut props = pumpkin_data::block_properties::SculkSensorLikeProperties::from_state_id(
            Block::SCULK_SENSOR.default_state.id,
        );
        props.waterlogged = waterlogged;
        props.to_state_id(&Block::SCULK_SENSOR)
    }
}

/// Vanilla `SculkBlock.canPlaceGrowth`: open space above and few growths nearby.
fn can_place_growth(world: &World, pos: &BlockPos) -> bool {
    let above = pos.up();
    let above_state = world.get_block_state(&above);
    let water_above = world.get_block(&above) == &Block::WATER
        && world
            .get_fluid_and_fluid_state(&above)
            .0
            .matches_type(&Fluid::WATER);
    if !above_state.is_air() && !water_above {
        return false;
    }
    let mut growths = 0;
    for pos in BlockPos::iterate(
        pos.offset(Vector3::new(-4, 0, -4)),
        pos.offset(Vector3::new(4, 2, 4)),
    ) {
        let block = world.get_block(&pos);
        if block == &Block::SCULK_SENSOR || block == &Block::SCULK_SHRIEKER {
            growths += 1;
        }
        if growths > 2 {
            return false;
        }
    }
    true
}

impl SculkSpreader {
    pub fn read_nbt(&mut self, nbt: &NbtCompound) {
        self.cursors.clear();
        if let Some(list) = nbt.get_list("cursors") {
            self.cursors.extend(
                list.iter()
                    .filter_map(NbtTag::extract_compound)
                    .filter_map(ChargeCursor::read_nbt)
                    .take(MAX_CURSORS),
            );
        }
    }

    pub fn write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_list(
            "cursors",
            self.cursors
                .iter()
                .map(|cursor| NbtTag::Compound(cursor.write_nbt()))
                .collect(),
        );
    }

    /// Vanilla `addCursors`: split the charge into cursors of at most 1000.
    pub fn add_cursors(&mut self, pos: BlockPos, mut charge: i32) {
        while charge > 0 {
            let part = charge.min(MAX_CHARGE);
            if self.cursors.len() < MAX_CURSORS {
                self.cursors.push(ChargeCursor::new(pos, part));
            }
            charge -= part;
        }
    }

    /// Vanilla `updateCursors`.
    pub fn update_cursors(&mut self, world: &Arc<World>, origin: &BlockPos, rng: &mut impl Rng) {
        if self.cursors.is_empty() {
            return;
        }
        let mut cursors = std::mem::take(&mut self.cursors);
        let mut kept: Vec<ChargeCursor> = Vec::new();
        let mut first_at: rustc_hash::FxHashMap<BlockPos, usize> = rustc_hash::FxHashMap::default();
        let mut totals: Vec<(BlockPos, i32)> = Vec::new();
        for cursor in &mut cursors {
            let d = cursor.pos.0 - origin.0;
            if d.x.abs().max(d.y.abs()).max(d.z.abs()) > MAX_CURSOR_DISTANCE {
                continue;
            }
            cursor.update(world, origin, rng, self);
            if cursor.charge <= 0 {
                world.sync_world_event(WorldEvent::ParticlesSculkCharge, cursor.pos, 0);
                continue;
            }
            let pos = cursor.pos;
            match totals.iter_mut().find(|(total_pos, _)| *total_pos == pos) {
                Some((_, total)) => *total += cursor.charge,
                None => totals.push((pos, cursor.charge)),
            }
            match first_at.get(&pos) {
                None => {
                    first_at.insert(pos, kept.len());
                    kept.push(cursor.clone());
                }
                Some(&index) if cursor.charge + kept[index].charge <= MAX_CHARGE => {
                    let mut merged = cursor.clone();
                    kept[index].merge_with(&mut merged);
                }
                Some(&index) => {
                    let lower = cursor.charge < kept[index].charge;
                    kept.push(cursor.clone());
                    if lower {
                        first_at.insert(pos, kept.len() - 1);
                    }
                }
            }
        }
        for (pos, total) in totals {
            let Some(facings) = first_at
                .get(&pos)
                .and_then(|&index| kept[index].facings.clone())
            else {
                continue;
            };
            if total > 0 {
                #[expect(clippy::cast_possible_truncation, reason = "vanilla truncates")]
                let level = (f64::from(total).ln_1p() / 2.299_999_952_316_284).floor() as i32 + 1;
                let data = (level << 6) + pack_faces(&facings);
                world.sync_world_event(WorldEvent::ParticlesSculkCharge, pos, data);
            }
        }
        self.cursors = kept;
    }
}
