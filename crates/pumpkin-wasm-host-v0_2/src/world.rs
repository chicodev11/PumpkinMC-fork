use super::{AccessorExt, in_active_context, run_blocking};
use crate::LockIgnorePoison;

use pumpkin_data::block_properties::NoteblockInstrument as InternalNoteblockInstrument;
use pumpkin_data::block_state::PistonBehavior;
use pumpkin_data::{BlockDirection as InternalBlockDirection, BlockId, BlockStateId};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::chunk::ChunkHeightmapType;
use pumpkin_world::chunk::io::Dirtiable;
use pumpkin_world::world::BlockFlags;
use std::sync::{Arc, Mutex as StdMutex};
use wasmtime::component::{Accessor, HasSelf, Resource};

use crate::pumpkin::plugin::common::Position as WitPosition;
use crate::pumpkin::plugin::game_rules::{
    GameRule as WitGameRule, GameRuleValue as WitGameRuleValue,
};
use crate::pumpkin::plugin::world::{
    Block as WitBlock, BlockDirection as WitBlockDirection, BlockEntityType,
    BlockFlags as WitBlockFlags, BlockPos as WitBlockPos, BlockState as WitBlockState,
    BlockStateInfo as WitBlockStateInfo, BoundingBox as WitBoundingBox, Chunk as WitChunk,
    Flammable as WitFlammable, NoteblockInstrument as WitNoteblockInstrument,
    PistonBehavior as WitPistonBehavior, RayTraceBlockResult as WitRayTraceBlockResult,
    RayTraceEntityResult as WitRayTraceEntityResult, WorldBorder as WitWorldBorder,
    WorldSpawnLocation as WitWorldSpawnLocation,
};
use crate::pumpkin::{self, plugin::world::World};
use pumpkin_core::block::entities::banner::BannerBlockEntity as InternalBannerBlockEntity;
use pumpkin_core::block::entities::barrel::BarrelBlockEntity as InternalBarrelBlockEntity;
use pumpkin_core::block::entities::beacon::BeaconBlockEntity as InternalBeaconBlockEntity;
use pumpkin_core::block::entities::bed::BedBlockEntity as InternalBedBlockEntity;
use pumpkin_core::block::entities::beehive::BeehiveBlockEntity as InternalBeehiveBlockEntity;
use pumpkin_core::block::entities::bell::BellBlockEntity as InternalBellBlockEntity;
use pumpkin_core::block::entities::blasting_furnace::BlastingFurnaceBlockEntity as InternalBlastingFurnaceBlockEntity;
use pumpkin_core::block::entities::brewing_stand::BrewingStandBlockEntity as InternalBrewingStandBlockEntity;
use pumpkin_core::block::entities::brushable_block::BrushableBlockBlockEntity as InternalBrushableBlockBlockEntity;
use pumpkin_core::block::entities::calibrated_sculk_sensor::CalibratedSculkSensorBlockEntity as InternalCalibratedSculkSensorBlockEntity;
use pumpkin_core::block::entities::campfire::CampfireBlockEntity as InternalCampfireBlockEntity;
use pumpkin_core::block::entities::chest::ChestBlockEntity as InternalChestBlockEntity;
use pumpkin_core::block::entities::chiseled_bookshelf::ChiseledBookshelfBlockEntity as InternalChiseledBookshelfBlockEntity;
use pumpkin_core::block::entities::command_block::CommandBlockEntity as InternalCommandBlockEntity;
use pumpkin_core::block::entities::comparator::ComparatorBlockEntity as InternalComparatorBlockEntity;
use pumpkin_core::block::entities::conduit::ConduitBlockEntity as InternalConduitBlockEntity;
use pumpkin_core::block::entities::copper_golem_statue::CopperGolemStatueBlockEntity as InternalCopperGolemStatueBlockEntity;
use pumpkin_core::block::entities::crafter::CrafterBlockEntity as InternalCrafterBlockEntity;
use pumpkin_core::block::entities::creaking_heart::CreakingHeartBlockEntity as InternalCreakingHeartBlockEntity;
use pumpkin_core::block::entities::daylight_detector::DaylightDetectorBlockEntity as InternalDaylightDetectorBlockEntity;
use pumpkin_core::block::entities::decorated_pot::DecoratedPotBlockEntity as InternalDecoratedPotBlockEntity;
use pumpkin_core::block::entities::dispenser::DispenserBlockEntity as InternalDispenserBlockEntity;
use pumpkin_core::block::entities::dropper::DropperBlockEntity as InternalDropperBlockEntity;
use pumpkin_core::block::entities::enchanting_table::EnchantingTableBlockEntity as InternalEnchantingTableBlockEntity;
use pumpkin_core::block::entities::end_gateway::EndGatewayBlockEntity as InternalEndGatewayBlockEntity;
use pumpkin_core::block::entities::end_portal::EndPortalBlockEntity as InternalEndPortalBlockEntity;
use pumpkin_core::block::entities::ender_chest::EnderChestBlockEntity as InternalEnderChestBlockEntity;
use pumpkin_core::block::entities::furnace::FurnaceBlockEntity as InternalFurnaceBlockEntity;
use pumpkin_core::block::entities::hanging_sign::HangingSignBlockEntity as InternalHangingSignBlockEntity;
use pumpkin_core::block::entities::hopper::HopperBlockEntity as InternalHopperBlockEntity;
use pumpkin_core::block::entities::jigsaw_block::JigsawBlockEntity as InternalJigsawBlockEntity;
use pumpkin_core::block::entities::jukebox::JukeboxBlockEntity as InternalJukeboxBlockEntity;
use pumpkin_core::block::entities::lectern::LecternBlockEntity as InternalLecternBlockEntity;
use pumpkin_core::block::entities::map::MapBlockEntity as InternalMapBlockEntity;
use pumpkin_core::block::entities::mob_spawner::MobSpawnerBlockEntity as InternalMobSpawnerBlockEntity;
use pumpkin_core::block::entities::piston::PistonBlockEntity as InternalPistonBlockEntity;
use pumpkin_core::block::entities::potent_sulfur::PotentSulfurBlockEntity as InternalPotentSulfurBlockEntity;
use pumpkin_core::block::entities::sculk_catalyst::SculkCatalystBlockEntity as InternalSculkCatalystBlockEntity;
use pumpkin_core::block::entities::sculk_sensor::SculkSensorBlockEntity as InternalSculkSensorBlockEntity;
use pumpkin_core::block::entities::sculk_shrieker::SculkShriekerBlockEntity as InternalSculkShriekerBlockEntity;
use pumpkin_core::block::entities::shelf::ShelfBlockEntity as InternalShelfBlockEntity;
use pumpkin_core::block::entities::shulker_box::ShulkerBoxBlockEntity as InternalShulkerBoxBlockEntity;
use pumpkin_core::block::entities::sign::SignBlockEntity as InternalSignBlockEntity;
use pumpkin_core::block::entities::skull::SkullBlockEntity as InternalSkullBlockEntity;
use pumpkin_core::block::entities::smoker::SmokerBlockEntity as InternalSmokerBlockEntity;
use pumpkin_core::block::entities::structure_block::StructureBlockBlockEntity as InternalStructureBlockBlockEntity;
use pumpkin_core::block::entities::test_block::TestBlockBlockEntity as InternalTestBlockBlockEntity;
use pumpkin_core::block::entities::test_instance_block::TestInstanceBlockBlockEntity as InternalTestInstanceBlockBlockEntity;
use pumpkin_core::block::entities::trapped_chest::TrappedChestBlockEntity as InternalTrappedChestBlockEntity;
use pumpkin_core::block::entities::trial_spawner::TrialSpawnerBlockEntity as InternalTrialSpawnerBlockEntity;
use pumpkin_core::block::entities::vault::VaultBlockEntity as InternalVaultBlockEntity;
use pumpkin_core::world::explosion::ExplosionInteraction;
use pumpkin_data::game_rules::{GameRule, GameRuleValue};
use pumpkin_wasm_host_common::state::ChunkBuffer;
use pumpkin_wasm_host_common::state::PluginHostState;

pub(crate) fn from_wit_game_rule(rule: WitGameRule) -> GameRule {
    // SAFETY: WIT GameRule and pumpkin_data::game_rules::GameRule have identical variant order
    unsafe { std::mem::transmute::<u8, GameRule>(rule as u8) }
}

pub(crate) fn to_wit_game_rule_value(value: &GameRuleValue<i64, bool>) -> WitGameRuleValue {
    match *value {
        GameRuleValue::Int(v) => {
            WitGameRuleValue::Int(v.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
        }
        GameRuleValue::Bool(v) => WitGameRuleValue::Bool(v),
    }
}

pub(crate) const fn from_wit_game_rule_value(value: WitGameRuleValue) -> GameRuleValue<i64, bool> {
    match value {
        WitGameRuleValue::Int(v) => GameRuleValue::Int(v as i64),
        WitGameRuleValue::Bool(v) => GameRuleValue::Bool(v),
    }
}

fn from_wit_block_flags(flags: WitBlockFlags) -> BlockFlags {
    let mut internal = BlockFlags::empty();
    if flags.contains(WitBlockFlags::NOTIFY_NEIGHBORS) {
        internal |= BlockFlags::NOTIFY_NEIGHBORS;
    }
    if flags.contains(WitBlockFlags::NOTIFY_LISTENERS) {
        internal |= BlockFlags::NOTIFY_LISTENERS;
    }
    if flags.contains(WitBlockFlags::FORCE_STATE) {
        internal |= BlockFlags::FORCE_STATE;
    }
    if flags.contains(WitBlockFlags::SKIP_DROPS) {
        internal |= BlockFlags::SKIP_DROPS;
    }
    if flags.contains(WitBlockFlags::MOVED) {
        internal |= BlockFlags::MOVED;
    }
    if flags.contains(WitBlockFlags::SKIP_REDSTONE_WIRE_STATE_REPLACEMENT) {
        internal |= BlockFlags::SKIP_REDSTONE_WIRE_STATE_REPLACEMENT;
    }
    if flags.contains(WitBlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK) {
        internal |= BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK;
    }
    if flags.contains(WitBlockFlags::SKIP_BLOCK_ADDED_CALLBACK) {
        internal |= BlockFlags::SKIP_BLOCK_ADDED_CALLBACK;
    }
    internal
}

fn world_from_state(
    state: &PluginHostState,
    world: &Resource<World>,
) -> wasmtime::Result<Arc<pumpkin_core::world::World>> {
    Ok(state.get(world)?.clone())
}

/// Returns the chunk data behind a chunk resource, failing if the chunk has unloaded.
///
/// A chunk resource only holds a [`Weak`](std::sync::Weak) reference, so the chunk can unload while
/// the plugin still has the handle.
fn add_populator(
    state: &PluginHostState,
    world: &Resource<World>,
    generator_id: u32,
    stage: pumpkin_world::generation::populator::PopulatorStage,
) -> wasmtime::Result<()> {
    let world_ref = state.get(world)?.clone();
    let (Some(plugin), Some(server)) = (
        state.plugin.as_ref().and_then(std::sync::Weak::upgrade),
        state.server.clone(),
    ) else {
        return Ok(());
    };
    let dimension = world_ref.dimension.clone();
    let generator = WasmChunkGenerator::new(
        generator_id,
        plugin,
        dimension.clone(),
        world_ref.level.seed.0,
        server,
    );
    pumpkin_world::generation::populator::register_at(
        stage,
        dimension.minecraft_name,
        Arc::new(WasmChunkPopulator(generator)),
    );
    Ok(())
}

fn loaded_chunk_data(
    state: &PluginHostState,
    chunk: &Resource<WitChunk>,
) -> wasmtime::Result<Arc<pumpkin_world::chunk::ChunkData>> {
    let (_, chunk_data) = state.get(chunk)?;
    chunk_data
        .upgrade()
        .ok_or_else(|| wasmtime::Error::msg("Chunk unloaded"))
}

async fn set_block_state_with_store(
    accessor: &Accessor<PluginHostState, HasSelf<PluginHostState>>,
    world: Resource<World>,
    pos: WitBlockPos,
    state: u16,
    update_flags: WitBlockFlags,
) -> wasmtime::Result<()> {
    let Some(state_id) = BlockStateId::new(state) else {
        return Err(wasmtime::Error::msg("Invalid BlockStateId"));
    };
    let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
    let internal_flags = from_wit_block_flags(update_flags);
    let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;

    run_blocking(accessor, move || {
        world.set_block_state(&internal_pos, state_id, internal_flags);
    })
    .await
}

pub(crate) const fn from_wit_structure_rotation(
    rotation: pumpkin::plugin::world::StructureRotation,
) -> pumpkin_data::Rotation {
    use pumpkin::plugin::world::StructureRotation;
    match rotation {
        StructureRotation::None => pumpkin_data::Rotation::None,
        StructureRotation::Clockwise90 => pumpkin_data::Rotation::Clockwise90,
        StructureRotation::Clockwise180 => pumpkin_data::Rotation::Rotate180,
        StructureRotation::Counterclockwise90 => pumpkin_data::Rotation::CounterClockwise90,
    }
}

pub(crate) const fn to_wasm_block_direction(dir: InternalBlockDirection) -> WitBlockDirection {
    match dir {
        InternalBlockDirection::Down => WitBlockDirection::Down,
        InternalBlockDirection::Up => WitBlockDirection::Up,
        InternalBlockDirection::North => WitBlockDirection::North,
        InternalBlockDirection::South => WitBlockDirection::South,
        InternalBlockDirection::West => WitBlockDirection::West,
        InternalBlockDirection::East => WitBlockDirection::East,
    }
}

pub(crate) const fn to_wit_noteblock_instrument(
    instr: InternalNoteblockInstrument,
) -> WitNoteblockInstrument {
    match instr {
        InternalNoteblockInstrument::Harp => WitNoteblockInstrument::Harp,
        InternalNoteblockInstrument::Basedrum => WitNoteblockInstrument::Basedrum,
        InternalNoteblockInstrument::Snare => WitNoteblockInstrument::Snare,
        InternalNoteblockInstrument::Hat => WitNoteblockInstrument::Hat,
        InternalNoteblockInstrument::Bass => WitNoteblockInstrument::Bass,
        InternalNoteblockInstrument::Flute => WitNoteblockInstrument::Flute,
        InternalNoteblockInstrument::Bell => WitNoteblockInstrument::Bell,
        InternalNoteblockInstrument::Guitar => WitNoteblockInstrument::Guitar,
        InternalNoteblockInstrument::Chime => WitNoteblockInstrument::Chime,
        InternalNoteblockInstrument::Xylophone => WitNoteblockInstrument::Xylophone,
        InternalNoteblockInstrument::IronXylophone => WitNoteblockInstrument::IronXylophone,
        InternalNoteblockInstrument::CowBell => WitNoteblockInstrument::CowBell,
        InternalNoteblockInstrument::Didgeridoo => WitNoteblockInstrument::Didgeridoo,
        InternalNoteblockInstrument::Bit => WitNoteblockInstrument::Bit,
        InternalNoteblockInstrument::Banjo => WitNoteblockInstrument::Banjo,
        InternalNoteblockInstrument::Pling => WitNoteblockInstrument::Pling,
        InternalNoteblockInstrument::Trumpet => WitNoteblockInstrument::Trumpet,
        InternalNoteblockInstrument::TrumpetExposed => WitNoteblockInstrument::TrumpetExposed,
        InternalNoteblockInstrument::TrumpetOxidized => WitNoteblockInstrument::TrumpetOxidized,
        InternalNoteblockInstrument::TrumpetWeathered => WitNoteblockInstrument::TrumpetWeathered,
        InternalNoteblockInstrument::Zombie => WitNoteblockInstrument::Zombie,
        InternalNoteblockInstrument::Skeleton => WitNoteblockInstrument::Skeleton,
        InternalNoteblockInstrument::Creeper => WitNoteblockInstrument::Creeper,
        InternalNoteblockInstrument::Dragon => WitNoteblockInstrument::Dragon,
        InternalNoteblockInstrument::WitherSkeleton => WitNoteblockInstrument::WitherSkeleton,
        InternalNoteblockInstrument::Piglin => WitNoteblockInstrument::Piglin,
        InternalNoteblockInstrument::CustomHead => WitNoteblockInstrument::CustomHead,
    }
}

pub(crate) const fn to_wit_bounding_box(
    bb: pumpkin_util::math::boundingbox::BoundingBox,
) -> WitBoundingBox {
    WitBoundingBox {
        min: (bb.min.x, bb.min.y, bb.min.z),
        max: (bb.max.x, bb.max.y, bb.max.z),
    }
}

pub(crate) fn to_wit_block(block: &pumpkin_data::Block) -> WitBlock {
    WitBlock {
        id: block.id.as_u16(),
        name: block.name.to_string(),
        hardness: block.hardness,
        blast_resistance: block.blast_resistance,
        map_color: block.map_color,
        slipperiness: block.slipperiness,
        velocity_multiplier: block.velocity_multiplier,
        jump_velocity_multiplier: block.jump_velocity_multiplier,
        item_id: block.item_id,
        default_state_id: block.default_state.id.as_u16(),
        state_ids: block.states.iter().map(|s| s.id.as_u16()).collect(),
        is_solid: block.is_solid(),
        is_air: block.is_air(),
        is_flammable: block.flammable.is_some(),
        flammable: block.flammable.as_ref().map(|f| WitFlammable {
            spread_chance: f.spread_chance,
            burn_chance: f.burn_chance,
        }),
    }
}

pub(crate) fn to_wit_block_state(
    state: &pumpkin_data::BlockState,
    pos: Option<&BlockPos>,
) -> WitBlockState {
    let dummy_pos = BlockPos::new(0, 0, 0);
    let internal_pos = pos.unwrap_or(&dummy_pos);
    let block = pumpkin_data::Block::from_state_id(state.id);
    let properties = block
        .properties(state.id)
        .map(|p| {
            p.to_props()
                .into_iter()
                .map(|prop| (prop.key_str().to_string(), prop.value_str().to_string()))
                .collect()
        })
        .unwrap_or_default();

    WitBlockState {
        id: state.id.as_u16(),
        block_id: block.id.as_u16(),
        block_name: block.name.to_string(),
        luminance: state.luminance,
        opacity: state.opacity,
        hardness: state.hardness,
        is_air: state.is_air(),
        is_liquid: state.is_liquid(),
        is_solid: state.is_solid(),
        is_full_cube: state.is_full_cube(),
        has_random_ticks: state.has_random_ticks(),
        piston_behavior: match state.piston_behavior {
            PistonBehavior::Normal => WitPistonBehavior::Normal,
            PistonBehavior::Destroy => WitPistonBehavior::Destroy,
            PistonBehavior::Block => WitPistonBehavior::Block,
            PistonBehavior::Ignore => WitPistonBehavior::Ignore,
            PistonBehavior::PushOnly => WitPistonBehavior::PushOnly,
        },
        burnable: state.burnable(),
        tool_required: state.tool_required(),
        sided_transparency: state.sided_transparency(),
        replaceable: state.replaceable(),
        is_solid_block: state.is_solid_block(),
        block_entity_type: state.block_entity_type,
        instrument: to_wit_noteblock_instrument(state.instrument),
        collision_shapes: state
            .get_block_collision_shapes_at(internal_pos)
            .map(to_wit_bounding_box)
            .collect(),
        outline_shapes: state
            .get_block_outline_shapes_at(internal_pos)
            .map(to_wit_bounding_box)
            .collect(),
        down_side_solid: state.is_side_solid(InternalBlockDirection::Down),
        up_side_solid: state.is_side_solid(InternalBlockDirection::Up),
        north_side_solid: state.is_side_solid(InternalBlockDirection::North),
        south_side_solid: state.is_side_solid(InternalBlockDirection::South),
        west_side_solid: state.is_side_solid(InternalBlockDirection::West),
        east_side_solid: state.is_side_solid(InternalBlockDirection::East),
        down_center_solid: state.is_center_solid(InternalBlockDirection::Down),
        up_center_solid: state.is_center_solid(InternalBlockDirection::Up),
        map_color: block.map_color,
        properties,
    }
}

// --- Trapping Helpers ---
pub trait WorldHostStateExt {
    fn get_wit_biome_v0_2(
        biome: &pumpkin_data::biome::Biome,
    ) -> wasmtime::Result<pumpkin::plugin::biomes::Biome>;
    fn get_wit_block_entity_v0_2(
        &mut self,
        block_entity: Arc<dyn pumpkin_core::block::entities::BlockEntity>,
    ) -> wasmtime::Result<Option<BlockEntityType>>;
}

impl WorldHostStateExt for PluginHostState {
    fn get_wit_biome_v0_2(
        biome: &pumpkin_data::biome::Biome,
    ) -> wasmtime::Result<pumpkin::plugin::biomes::Biome> {
        to_wit_biome(biome)
    }

    #[expect(
        unused_assignments,
        reason = "the tail of the macro output will put the Arc back into the `be` variable"
    )]
    fn get_wit_block_entity_v0_2(
        &mut self,
        block_entity: Arc<dyn pumpkin_core::block::entities::BlockEntity>,
    ) -> wasmtime::Result<Option<BlockEntityType>> {
        let mut be = block_entity as _;
        macro_rules! match_be {
            ($( $internal_type:ty => $variant:ident ),* $(,)?) => {
                $(
                    // these kind of chains can be expressed in two different ways, I chose this over
                    // if (&be as &Any).is::<T>() { unsafe { Arc::downcast::<T>(be).unwrap_unchecked() } }
                    // eventually it would be great if plugin authors had to manually request a specific be type
                    match Arc::downcast::<$internal_type>(be) {
                        Ok(entity) => return Ok(Some(BlockEntityType::$variant(self.add(entity)?))),
                        Err(entity) => be = entity,
                    }
                )*
            };
        }

        match_be! {
            InternalCommandBlockEntity => CommandBlockEntity,
            InternalSignBlockEntity => SignBlockEntity,
            InternalHangingSignBlockEntity => HangingSignBlockEntity,
            InternalJukeboxBlockEntity => JukeboxBlockEntity,
            InternalChestBlockEntity => ChestBlockEntity,
            InternalTrappedChestBlockEntity => TrappedChestBlockEntity,
            InternalMobSpawnerBlockEntity => MobSpawnerBlockEntity,
            InternalMapBlockEntity => MapBlockEntity,
            InternalBannerBlockEntity => BannerBlockEntity,
            InternalBarrelBlockEntity => BarrelBlockEntity,
            InternalBeaconBlockEntity => BeaconBlockEntity,
            InternalBedBlockEntity => BedBlockEntity,
            InternalBeehiveBlockEntity => BeehiveBlockEntity,
            InternalBellBlockEntity => BellBlockEntity,
            InternalBlastingFurnaceBlockEntity => BlastingFurnaceBlockEntity,
            InternalBrewingStandBlockEntity => BrewingStandBlockEntity,
            InternalBrushableBlockBlockEntity => BrushableBlockBlockEntity,
            InternalCalibratedSculkSensorBlockEntity => CalibratedSculkSensorBlockEntity,
            InternalCampfireBlockEntity => CampfireBlockEntity,
            InternalChiseledBookshelfBlockEntity => ChiseledBookshelfBlockEntity,
            InternalComparatorBlockEntity => ComparatorBlockEntity,
            InternalConduitBlockEntity => ConduitBlockEntity,
            InternalCopperGolemStatueBlockEntity => CopperGolemStatueBlockEntity,
            InternalCrafterBlockEntity => CrafterBlockEntity,
            InternalCreakingHeartBlockEntity => CreakingHeartBlockEntity,
            InternalDaylightDetectorBlockEntity => DaylightDetectorBlockEntity,
            InternalDecoratedPotBlockEntity => DecoratedPotBlockEntity,
            InternalDispenserBlockEntity => DispenserBlockEntity,
            InternalDropperBlockEntity => DropperBlockEntity,
            InternalEnchantingTableBlockEntity => EnchantingTableBlockEntity,
            InternalEndGatewayBlockEntity => EndGatewayBlockEntity,
            InternalEndPortalBlockEntity => EndPortalBlockEntity,
            InternalEnderChestBlockEntity => EnderChestBlockEntity,
            InternalFurnaceBlockEntity => FurnaceBlockEntity,
            InternalHopperBlockEntity => HopperBlockEntity,
            InternalJigsawBlockEntity => JigsawBlockEntity,
            InternalLecternBlockEntity => LecternBlockEntity,
            InternalPistonBlockEntity => PistonBlockEntity,
            InternalPotentSulfurBlockEntity => PotentSulfurBlockEntity,
            InternalSculkCatalystBlockEntity => SculkCatalystBlockEntity,
            InternalSculkSensorBlockEntity => SculkSensorBlockEntity,
            InternalSculkShriekerBlockEntity => SculkShriekerBlockEntity,
            InternalShelfBlockEntity => ShelfBlockEntity,
            InternalShulkerBoxBlockEntity => ShulkerBoxBlockEntity,
            InternalSkullBlockEntity => SkullBlockEntity,
            InternalSmokerBlockEntity => SmokerBlockEntity,
            InternalStructureBlockBlockEntity => StructureBlockBlockEntity,
            InternalTestBlockBlockEntity => TestBlockBlockEntity,
            InternalTestInstanceBlockBlockEntity => TestInstanceBlockBlockEntity,
            InternalTrialSpawnerBlockEntity => TrialSpawnerBlockEntity,
            InternalVaultBlockEntity => VaultBlockEntity,
        }

        Ok(None)
    }
}

impl pumpkin::plugin::world::Host for PluginHostState {
    fn resolve_block_state(
        &mut self,
        name: String,
        properties: Vec<(String, String)>,
    ) -> wasmtime::Result<Option<u16>> {
        let Some(block) = pumpkin_data::Block::from_name(&name) else {
            return Ok(None);
        };

        if properties.is_empty() {
            return Ok(Some(block.default_state.id.as_u16()));
        }

        let props: Vec<pumpkin_data::Property> = properties
            .iter()
            .filter_map(|(k, v)| pumpkin_data::Property::from_str_pair(k, v))
            .collect();
        // from_properties/from_value panics on unknown property values,
        // so catch panics to avoid crashing on schematics from other MC versions.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let block_props = block.from_properties(&props);
            block_props.to_state_id(block).as_u16()
        }));
        Ok(result.ok())
    }

    fn block_state_to_info(
        &mut self,
        state_id: u16,
    ) -> wasmtime::Result<Option<WitBlockStateInfo>> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let bsid = BlockStateId::new_or_air(state_id);
            let block = pumpkin_data::Block::from_state_id(bsid);
            let name = block.registry_name();
            let properties = block
                .properties(bsid)
                .map(|p| {
                    p.to_props()
                        .into_iter()
                        .map(|prop| (prop.key_str().to_string(), prop.value_str().to_string()))
                        .collect()
                })
                .unwrap_or_default();
            WitBlockStateInfo { name, properties }
        }));
        Ok(result.ok())
    }

    fn get_block_by_id(&mut self, id: u16) -> wasmtime::Result<Option<WitBlock>> {
        let block_id = BlockId::new(id);
        Ok(block_id.map(|id| to_wit_block(pumpkin_data::Block::from_id(id))))
    }

    fn get_block_by_name(&mut self, name: String) -> wasmtime::Result<Option<WitBlock>> {
        Ok(pumpkin_data::Block::from_name(&name).map(to_wit_block))
    }

    fn get_all_blocks(&mut self) -> wasmtime::Result<Vec<WitBlock>> {
        let mut blocks = Vec::with_capacity(BlockId::COUNT as usize);
        for raw_id in 0..BlockId::COUNT {
            if let Some(id) = BlockId::new(raw_id) {
                blocks.push(to_wit_block(pumpkin_data::Block::from_id(id)));
            }
        }
        Ok(blocks)
    }

    fn get_all_block_names(&mut self) -> wasmtime::Result<Vec<String>> {
        let mut names = Vec::with_capacity(BlockId::COUNT as usize);
        for raw_id in 0..BlockId::COUNT {
            if let Some(id) = BlockId::new(raw_id) {
                names.push(pumpkin_data::Block::from_id(id).name.to_string());
            }
        }
        Ok(names)
    }

    fn get_block_count(&mut self) -> wasmtime::Result<u32> {
        Ok(BlockId::COUNT as u32)
    }

    fn get_block_state_count(&mut self) -> wasmtime::Result<u32> {
        Ok(BlockStateId::COUNT as u32)
    }

    fn get_states_for_block(&mut self, block: WitBlock) -> wasmtime::Result<Vec<WitBlockState>> {
        let block_id = BlockId::new_or_air(block.id);
        let block_ref = pumpkin_data::Block::from_id(block_id);
        Ok(block_ref
            .states
            .iter()
            .map(|s| to_wit_block_state(s, None))
            .collect())
    }

    fn get_states_for_block_id(&mut self, block_id: u16) -> wasmtime::Result<Vec<WitBlockState>> {
        let Some(id) = BlockId::new(block_id) else {
            return Ok(Vec::new());
        };
        let block_ref = pumpkin_data::Block::from_id(id);
        Ok(block_ref
            .states
            .iter()
            .map(|s| to_wit_block_state(s, None))
            .collect())
    }

    fn get_state_ids_for_block_id(&mut self, block_id: u16) -> wasmtime::Result<Vec<u16>> {
        let Some(id) = BlockId::new(block_id) else {
            return Ok(Vec::new());
        };
        let block_ref = pumpkin_data::Block::from_id(id);
        Ok(block_ref.states.iter().map(|s| s.id.as_u16()).collect())
    }

    fn get_block_properties(&mut self, state_id: u16) -> wasmtime::Result<Vec<(String, String)>> {
        let bsid = BlockStateId::new_or_air(state_id);
        let block = pumpkin_data::Block::from_state_id(bsid);
        let props = block
            .properties(bsid)
            .map(|p| {
                p.to_props()
                    .into_iter()
                    .map(|prop| (prop.key_str().to_string(), prop.value_str().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(props)
    }

    fn get_block_from_state_id(&mut self, state_id: u16) -> wasmtime::Result<Option<WitBlock>> {
        let bsid = BlockStateId::new(state_id);
        Ok(bsid.map(|id| to_wit_block(pumpkin_data::Block::from_state_id(id))))
    }

    fn get_block_from_state(&mut self, state: WitBlockState) -> wasmtime::Result<WitBlock> {
        let bsid = BlockStateId::new_or_air(state.id);
        Ok(to_wit_block(pumpkin_data::Block::from_state_id(bsid)))
    }

    fn get_default_state_from_block(&mut self, block: WitBlock) -> wasmtime::Result<WitBlockState> {
        let block_id = BlockId::new_or_air(block.id);
        let block_ref = pumpkin_data::Block::from_id(block_id);
        Ok(to_wit_block_state(block_ref.default_state, None))
    }

    fn get_default_state_from_block_id(
        &mut self,
        block_id: u16,
    ) -> wasmtime::Result<Option<WitBlockState>> {
        let block_id = BlockId::new(block_id);
        Ok(block_id.map(|id| {
            let block = pumpkin_data::Block::from_id(id);
            to_wit_block_state(block.default_state, None)
        }))
    }

    fn get_block_state_by_id(&mut self, state_id: u16) -> wasmtime::Result<Option<WitBlockState>> {
        let bsid = BlockStateId::new(state_id);
        Ok(bsid.map(|id| {
            let state = pumpkin_data::BlockState::from_id(id);
            to_wit_block_state(state, None)
        }))
    }

    fn is_block_in_tag(&mut self, block_name: String, tag: String) -> wasmtime::Result<bool> {
        use pumpkin_data::tag::Taggable;
        let Some(block) = pumpkin_data::Block::from_name(&block_name) else {
            return Ok(false);
        };
        Ok(block.is_tagged_with(&tag).unwrap_or(false))
    }

    fn is_block_id_in_tag(&mut self, block_id: u16, tag: String) -> wasmtime::Result<bool> {
        use pumpkin_data::tag::Taggable;
        let Some(id) = BlockId::new(block_id) else {
            return Ok(false);
        };
        let block = pumpkin_data::Block::from_id(id);
        Ok(block.is_tagged_with(&tag).unwrap_or(false))
    }

    fn get_blocks_in_tag(&mut self, tag: String) -> wasmtime::Result<Vec<String>> {
        use pumpkin_data::tag::Taggable;
        Ok(pumpkin_data::Block::get_tag_values(&tag)
            .map(|vals| vals.iter().map(|s| (*s).to_string()).collect())
            .unwrap_or_default())
    }

    fn get_block_ids_in_tag(&mut self, tag: String) -> wasmtime::Result<Vec<u16>> {
        let tag = tag.strip_prefix('#').unwrap_or(&tag);
        let ids = pumpkin_data::tag::get_tag_ids(pumpkin_data::tag::RegistryKey::Block, tag)
            .or_else(|| {
                if !tag.contains(':') {
                    let full = format!("minecraft:{tag}");
                    pumpkin_data::tag::get_tag_ids(pumpkin_data::tag::RegistryKey::Block, &full)
                } else if let Some(stripped) = tag.strip_prefix("minecraft:") {
                    pumpkin_data::tag::get_tag_ids(pumpkin_data::tag::RegistryKey::Block, stripped)
                } else {
                    None
                }
            });
        Ok(ids.map(<[u16]>::to_vec).unwrap_or_default())
    }

    fn get_block_tags(&mut self, block_name: String) -> wasmtime::Result<Vec<String>> {
        let Some(block) = pumpkin_data::Block::from_name(&block_name) else {
            return Ok(Vec::new());
        };
        let block_id = block.id.as_u16();
        Ok(
            pumpkin_data::tag::get_latest_map(pumpkin_data::tag::RegistryKey::Block)
                .entries()
                .filter(|(_, tag)| tag.1.contains(&block_id))
                .map(|(name, _)| (*name).to_string())
                .collect(),
        )
    }

    fn is_entity_type_in_tag(
        &mut self,
        entity_type_name: String,
        tag: String,
    ) -> wasmtime::Result<bool> {
        use pumpkin_data::tag::Taggable;
        let name = entity_type_name
            .strip_prefix("minecraft:")
            .unwrap_or(&entity_type_name);
        let Some(entity_type) = pumpkin_data::entity::EntityType::from_name(name) else {
            return Ok(false);
        };
        Ok(entity_type.is_tagged_with(&tag).unwrap_or(false))
    }

    fn get_entity_types_in_tag(&mut self, tag: String) -> wasmtime::Result<Vec<String>> {
        use pumpkin_data::tag::Taggable;
        Ok(pumpkin_data::entity::EntityType::get_tag_values(&tag)
            .map(|vals| vals.iter().map(|s| (*s).to_string()).collect())
            .unwrap_or_default())
    }

    fn get_entity_type_tags(&mut self, entity_type_name: String) -> wasmtime::Result<Vec<String>> {
        let name = entity_type_name
            .strip_prefix("minecraft:")
            .unwrap_or(&entity_type_name);
        let Some(entity_type) = pumpkin_data::entity::EntityType::from_name(name) else {
            return Ok(Vec::new());
        };
        let entity_id = entity_type.id;
        Ok(
            pumpkin_data::tag::get_latest_map(pumpkin_data::tag::RegistryKey::EntityType)
                .entries()
                .filter(|(_, tag)| tag.1.contains(&entity_id))
                .map(|(tag_name, _)| (*tag_name).to_string())
                .collect(),
        )
    }
}
impl pumpkin::plugin::particles::Host for PluginHostState {}
impl pumpkin::plugin::sounds::Host for PluginHostState {}

impl pumpkin::plugin::world::HostWorld for PluginHostState {
    fn get_id(&mut self, world: Resource<World>) -> wasmtime::Result<String> {
        Ok(self.get(&world)?.get_world_name().to_string())
    }

    fn get_uuid(
        &mut self,
        world: Resource<World>,
    ) -> wasmtime::Result<crate::pumpkin::plugin::uuid::Uuid> {
        let uuid = self.get(&world)?.uuid;
        Ok(<crate::pumpkin::plugin::uuid::Uuid as crate::uuid::UuidExt>::to_wit(&uuid))
    }

    fn get_seed(&mut self, world: Resource<World>) -> wasmtime::Result<i64> {
        Ok(self.get(&world)?.level.seed.0 as i64)
    }

    fn get_border(&mut self, world: Resource<World>) -> wasmtime::Result<Resource<WitWorldBorder>> {
        self.get_world_border(world)
    }

    fn get_world_border(
        &mut self,
        world: Resource<World>,
    ) -> wasmtime::Result<Resource<WitWorldBorder>> {
        let world_res = self.get(&world)?;
        self.add(world_res.clone())
    }

    fn get_spawn_location(
        &mut self,
        world: Resource<World>,
    ) -> wasmtime::Result<WitWorldSpawnLocation> {
        let (pos, yaw, pitch) = self.get(&world)?.get_spawn_location();
        Ok(WitWorldSpawnLocation {
            pos: WitBlockPos {
                x: pos.0.x,
                y: pos.0.y,
                z: pos.0.z,
            },
            yaw,
            pitch,
        })
    }

    fn set_spawn_location(
        &mut self,
        world: Resource<World>,
        location: WitWorldSpawnLocation,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let current_info = world_ref.level_info.load();
        let mut new_info = (**current_info).clone();
        new_info.spawn_x = location.pos.x;
        new_info.spawn_y = location.pos.y;
        new_info.spawn_z = location.pos.z;
        new_info.spawn_yaw = location.yaw;
        new_info.spawn_pitch = location.pitch;
        world_ref.level_info.store(Arc::new(new_info));
        Ok(())
    }

    fn resend_chunks(&mut self, world: Resource<World>) -> wasmtime::Result<u32> {
        let world_ref = self.get(&world)?;
        Ok(u32::try_from(world_ref.resend_chunks_to_players()).unwrap_or(u32::MAX))
    }

    fn is_chunk_loaded(
        &mut self,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<bool> {
        let world_ref = self.get(&world)?;
        let pos = pumpkin_util::math::vector2::Vector2::new(x, z);
        Ok(world_ref.level.is_chunk_loaded(&pos))
    }

    fn is_chunk_force_loaded(
        &mut self,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<bool> {
        let world_ref = self.get(&world)?;
        let pos = pumpkin_util::math::vector2::Vector2::new(x, z);
        Ok(world_ref.forced_chunks.lock_ignore_poison().contains(&pos))
    }

    fn set_chunk_force_loaded(
        &mut self,
        world: Resource<World>,
        x: i32,
        z: i32,
        forced: bool,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?.clone();
        let pos = pumpkin_util::math::vector2::Vector2::new(x, z);
        let changed = {
            let mut forced_chunks = world_ref.forced_chunks.lock_ignore_poison();
            if forced {
                forced_chunks.insert(pos)
            } else {
                forced_chunks.remove(&pos)
            }
        };
        if changed {
            let mut loading = world_ref.level.chunk_loading.lock_ignore_poison();
            if forced {
                loading.add_force_ticket(pos);
            } else {
                loading.remove_force_ticket(pos);
            }
            loading.send_change();
            drop(loading);
            world_ref.update_active_chunks();
        }
        Ok(())
    }

    fn get_force_loaded_chunks(
        &mut self,
        world: Resource<World>,
    ) -> wasmtime::Result<Vec<(i32, i32)>> {
        let world_ref = self.get(&world)?;
        let forced = world_ref.forced_chunks.lock_ignore_poison();
        Ok(forced.iter().map(|pos| (pos.x, pos.y)).collect())
    }

    fn add_plugin_chunk_ticket(
        &mut self,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<bool> {
        let name = self.name.clone().unwrap_or_default();
        let world = self.get(&world)?.clone();
        Ok(world.add_plugin_chunk_ticket(&name, pumpkin_util::math::vector2::Vector2::new(x, z)))
    }

    fn remove_plugin_chunk_ticket(
        &mut self,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<bool> {
        let name = self.name.clone().unwrap_or_default();
        let world = self.get(&world)?.clone();
        Ok(
            world
                .remove_plugin_chunk_ticket(&name, pumpkin_util::math::vector2::Vector2::new(x, z)),
        )
    }

    fn get_plugin_chunk_tickets(
        &mut self,
        world: Resource<World>,
    ) -> wasmtime::Result<Vec<(i32, i32)>> {
        let name = self.name.clone().unwrap_or_default();
        let world = self.get(&world)?;
        Ok(world
            .plugin_chunk_tickets_of(&name)
            .into_iter()
            .map(|pos| (pos.x, pos.y))
            .collect())
    }

    fn get_block_power(
        &mut self,
        world: Resource<World>,
        pos: crate::pumpkin::plugin::common::BlockPos,
    ) -> wasmtime::Result<u8> {
        let world = self.get(&world)?;
        Ok(pumpkin_core::block::blocks::redstone::best_neighbor_signal(
            world,
            &wit_block_pos(&pos),
        ))
    }

    fn is_block_powered(
        &mut self,
        world: Resource<World>,
        pos: crate::pumpkin::plugin::common::BlockPos,
    ) -> wasmtime::Result<bool> {
        let world = self.get(&world)?;
        Ok(
            pumpkin_core::block::blocks::redstone::direct_signal_to(world, &wit_block_pos(&pos))
                > 0,
        )
    }

    fn is_block_indirectly_powered(
        &mut self,
        world: Resource<World>,
        pos: crate::pumpkin::plugin::common::BlockPos,
    ) -> wasmtime::Result<bool> {
        let world = self.get(&world)?;
        Ok(
            pumpkin_core::block::blocks::redstone::block_receives_redstone_power(
                world,
                &wit_block_pos(&pos),
            ),
        )
    }

    fn get_loaded_chunks(
        &mut self,
        world: Resource<World>,
    ) -> wasmtime::Result<Vec<Resource<WitChunk>>> {
        let world_provider = self.get(&world)?.clone();
        let chunks: Vec<_> = world_provider
            .level
            .loaded_chunks
            .iter()
            .map(|entry| Arc::downgrade(entry.value()))
            .collect();
        let mut results = Vec::with_capacity(chunks.len());
        for weak_chunk in chunks {
            results.push(self.add((Arc::clone(&world_provider), weak_chunk))?);
        }
        Ok(results)
    }

    fn get_entity_count(&mut self, world: Resource<World>) -> wasmtime::Result<u32> {
        let world_ref = self.get(&world)?;
        Ok(u32::try_from(world_ref.entities.load().len()).unwrap_or(u32::MAX))
    }

    fn get_player_count(&mut self, world: Resource<World>) -> wasmtime::Result<u32> {
        let world_ref = self.get(&world)?;
        Ok(u32::try_from(world_ref.players.load().len()).unwrap_or(u32::MAX))
    }

    fn get_loaded_chunk_count(&mut self, world: Resource<World>) -> wasmtime::Result<u32> {
        let world_ref = self.get(&world)?;
        Ok(world_ref.level.loaded_chunk_count() as u32)
    }

    fn get_chunk(
        &mut self,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<Option<Resource<WitChunk>>> {
        let world_res = self.get(&world)?;
        let world_provider = world_res.clone();
        let pos = pumpkin_util::math::vector2::Vector2::new(x, z);

        let chunk = world_provider
            .level
            .loaded_chunks
            .get(&pos)
            .map(|c| c.value().clone());
        if let Some(chunk) = chunk {
            let res = self.add((world_provider, std::sync::Arc::downgrade(&chunk)))?;
            Ok(Some(res))
        } else {
            Ok(None)
        }
    }

    fn schedule_block_tick(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
        delay: u8,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let block = world_ref.get_block(&internal_pos);
        world_ref.schedule_block_tick(
            block,
            internal_pos,
            delay.max(1),
            pumpkin_world::tick::TickPriority::Normal,
        );
        Ok(())
    }

    fn get_block_state_id(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<u16> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        Ok(world_ref.get_block_state_id(&internal_pos).as_u16())
    }

    fn get_block_state(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<WitBlockState> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let state = world_ref.get_block_state(&internal_pos);
        Ok(to_wit_block_state(state, Some(&internal_pos)))
    }

    fn get_block(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<WitBlock> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let state = world_ref.get_block_state(&internal_pos);
        let block = pumpkin_data::Block::from_state_id(state.id);
        Ok(to_wit_block(block))
    }

    fn get_block_id(&mut self, world: Resource<World>, pos: WitBlockPos) -> wasmtime::Result<u16> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let state = world_ref.get_block_state(&internal_pos);
        Ok(pumpkin_data::BlockId::from_state_id(state.id).as_u16())
    }

    fn get_time_of_day(&mut self, world: Resource<World>) -> wasmtime::Result<u64> {
        Ok(self.get(&world)?.get_time_of_day() as u64)
    }

    fn set_time_of_day(&mut self, world: Resource<World>, time: u64) -> wasmtime::Result<()> {
        self.get(&world)?.set_time_of_day(time as i64);
        Ok(())
    }

    fn get_world_age(&mut self, world: Resource<World>) -> wasmtime::Result<u64> {
        Ok(self.get(&world)?.get_world_age() as u64)
    }

    fn get_dimension(&mut self, world: Resource<World>) -> wasmtime::Result<String> {
        Ok(self.get(&world)?.dimension.minecraft_name.to_string())
    }

    fn get_top_block_y(&mut self, world: Resource<World>, x: i32, z: i32) -> wasmtime::Result<i32> {
        Ok(self
            .get(&world)?
            .get_top_block(pumpkin_util::math::vector2::Vector2::new(x, z)))
    }

    fn get_motion_blocking_height(
        &mut self,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<i32> {
        Ok(self
            .get(&world)?
            .get_heightmap_height(ChunkHeightmapType::MotionBlocking, x, z))
    }

    fn is_raining(&mut self, world: Resource<World>) -> wasmtime::Result<bool> {
        Ok(self.get(&world)?.is_raining())
    }

    fn is_thundering(&mut self, world: Resource<World>) -> wasmtime::Result<bool> {
        Ok(self.get(&world)?.is_thundering())
    }

    fn get_weather_duration(&mut self, world: Resource<World>) -> wasmtime::Result<i32> {
        let world_ref = self.get(&world)?;
        let weather = world_ref.weather.lock_ignore_poison();
        Ok(weather.rain_time)
    }

    fn set_weather_duration(
        &mut self,
        world: Resource<World>,
        duration: i32,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let mut weather = world_ref.weather.lock_ignore_poison();
        weather.rain_time = duration;
        Ok(())
    }

    fn get_thunder_duration(&mut self, world: Resource<World>) -> wasmtime::Result<i32> {
        let world_ref = self.get(&world)?;
        let weather = world_ref.weather.lock_ignore_poison();
        Ok(weather.thunder_time)
    }

    fn set_thunder_duration(
        &mut self,
        world: Resource<World>,
        duration: i32,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let mut weather = world_ref.weather.lock_ignore_poison();
        weather.thunder_time = duration;
        Ok(())
    }

    fn get_clear_weather_duration(&mut self, world: Resource<World>) -> wasmtime::Result<i32> {
        let world_ref = self.get(&world)?;
        let weather = world_ref.weather.lock_ignore_poison();
        Ok(weather.clear_weather_time)
    }

    fn set_clear_weather_duration(
        &mut self,
        world: Resource<World>,
        duration: i32,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let mut weather = world_ref.weather.lock_ignore_poison();
        weather.clear_weather_time = duration;
        Ok(())
    }

    fn broadcast_system_message(
        &mut self,
        world: Resource<World>,
        message: Resource<pumpkin::plugin::text::TextComponent>,
        overlay: bool,
    ) -> wasmtime::Result<()> {
        let msg = self.take(message)?;
        self.get(&world)?.broadcast_system_message(&msg, overlay);
        Ok(())
    }

    fn get_scoreboard(
        &mut self,
        world: Resource<World>,
    ) -> wasmtime::Result<Resource<pumpkin::plugin::scoreboard::Scoreboard>> {
        let world_provider = self.get(&world)?.clone();
        self.add(pumpkin_wasm_host_common::state::ScoreboardProvider::World(
            world_provider,
        ))
    }

    fn play_sound(
        &mut self,
        world: Resource<World>,
        sound: pumpkin::plugin::sounds::Sound,
        category: pumpkin::plugin::sounds::SoundCategory,
        pos: pumpkin::plugin::common::Position,
        volume: f32,
        pitch: f32,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let sound_name = format!("{sound:?}").to_lowercase().replace('_', ".");
        let sound_data = pumpkin_data::sound::Sound::from_name(&sound_name)
            .ok_or_else(|| wasmtime::Error::msg(format!("Unknown sound: {sound_name}")))?;

        let internal_category = from_wit_sound_category(category);

        world_ref.play_sound_raw(
            sound_data as u16,
            internal_category,
            &pumpkin_util::math::vector3::Vector3::new(pos.0, pos.1, pos.2),
            volume,
            pitch,
        );
        Ok(())
    }

    fn play_custom_sound(
        &mut self,
        world: Resource<World>,
        sound_name: String,
        category: pumpkin::plugin::sounds::SoundCategory,
        pos: pumpkin::plugin::common::Position,
        volume: f32,
        pitch: f32,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let internal_category = from_wit_sound_category(category);
        world_ref.play_custom_sound(
            &sound_name,
            internal_category,
            &pumpkin_util::math::vector3::Vector3::new(pos.0, pos.1, pos.2),
            volume,
            pitch,
        );
        Ok(())
    }

    fn play_custom_sound_from_entity(
        &mut self,
        world: Resource<World>,
        sound_name: String,
        category: pumpkin::plugin::sounds::SoundCategory,
        entity: Resource<pumpkin::plugin::world::Entity>,
        volume: f32,
        pitch: f32,
    ) -> wasmtime::Result<()> {
        let entity_id = self.get(&entity)?.get_entity().entity_id;
        let world_ref = self.get(&world)?;
        world_ref.play_custom_sound_from_entity(
            &sound_name,
            from_wit_sound_category(category),
            entity_id,
            volume,
            pitch,
        );
        Ok(())
    }

    fn spawn_particle(
        &mut self,
        world: Resource<World>,
        particle: pumpkin::plugin::particles::Particle,
        pos: pumpkin::plugin::common::Position,
        offset: pumpkin::plugin::common::Position,
        max_speed: f32,
        count: i32,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let particle_data =
            pumpkin_data::particle::Particle::from_id(particle as u16).ok_or_else(|| {
                wasmtime::Error::msg(format!("Unknown particle ID: {}", particle as u16))
            })?;

        world_ref.spawn_particle(
            pumpkin_util::math::vector3::Vector3::new(pos.0, pos.1, pos.2),
            pumpkin_util::math::vector3::Vector3::new(
                offset.0 as f32,
                offset.1 as f32,
                offset.2 as f32,
            ),
            max_speed,
            count,
            particle_data,
        );
        Ok(())
    }

    fn get_sea_level(&mut self, world: Resource<World>) -> wasmtime::Result<i32> {
        Ok(self.get(&world)?.sea_level)
    }

    fn get_min_y(&mut self, world: Resource<World>) -> wasmtime::Result<i32> {
        Ok(self.get(&world)?.min_y)
    }

    fn get_max_y(&mut self, world: Resource<World>) -> wasmtime::Result<i32> {
        let world_ref = self.get(&world)?;
        Ok(world_ref.min_y + world_ref.dimension.height)
    }

    fn get_height(&mut self, world: Resource<World>) -> wasmtime::Result<u32> {
        Ok(self.get(&world)?.dimension.height.max(0) as u32)
    }

    fn get_logical_height(&mut self, world: Resource<World>) -> wasmtime::Result<u32> {
        Ok(self.get(&world)?.dimension.logical_height.max(0) as u32)
    }

    fn get_sky_light(&mut self, world: Resource<World>, pos: WitBlockPos) -> wasmtime::Result<u8> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        Ok(world_ref.get_sky_light_level(&internal_pos))
    }

    fn set_sky_light(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
        level: u8,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        world_ref.set_sky_light_level(&internal_pos, level);
        Ok(())
    }

    fn get_block_light(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<u8> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        Ok(world_ref.get_block_light_level(&internal_pos).unwrap_or(0))
    }

    fn get_light_level(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<u8> {
        let world_ref = self.get(&world)?;
        Ok(world_ref.get_max_local_raw_brightness(&BlockPos::new(pos.x, pos.y, pos.z)))
    }

    fn get_raw_brightness(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
        sky_darken: u8,
    ) -> wasmtime::Result<u8> {
        let world_ref = self.get(&world)?;
        Ok(world_ref.get_raw_brightness(&BlockPos::new(pos.x, pos.y, pos.z), sky_darken))
    }

    fn set_block_light(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
        level: u8,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        world_ref.set_block_light_level(&internal_pos, level);
        Ok(())
    }

    fn get_biome(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<pumpkin::plugin::biomes::Biome> {
        let world_ref = self.get(&world)?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let biome = world_ref.get_biome(&internal_pos);

        Self::get_wit_biome_v0_2(biome)
    }

    fn set_biome(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
        biome: pumpkin::plugin::biomes::Biome,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?;
        let internal_biome = from_wit_biome(biome)?;
        let chunk_pos =
            pumpkin_util::math::vector2::Vector2::new(pos.x.div_euclid(16), pos.z.div_euclid(16));
        if let Some(chunk) = world_ref.level.loaded_chunks.get(&chunk_pos) {
            let y_rel = pos.y - chunk.section.min_y;
            if y_rel >= 0 {
                let rel_x = (pos.x.rem_euclid(16) as usize >> 2) & 3;
                let rel_y = y_rel as usize >> 2;
                let rel_z = (pos.z.rem_euclid(16) as usize >> 2) & 3;
                chunk
                    .section
                    .set_relative_biome(rel_x, rel_y, rel_z, internal_biome.id);
                chunk.mark_dirty(true);
            }
        }
        Ok(())
    }

    fn set_biome_by_name(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
        name: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let biome_id = match biome_id_by_name(&name) {
            Ok(id) => id,
            Err(error) => return Ok(Err(error)),
        };
        let world_ref = self.get(&world)?;
        let chunk_pos =
            pumpkin_util::math::vector2::Vector2::new(pos.x.div_euclid(16), pos.z.div_euclid(16));
        let Some(chunk) = world_ref.level.loaded_chunks.get(&chunk_pos) else {
            return Ok(Err("chunk is not loaded".to_string()));
        };
        let y_rel = pos.y - chunk.section.min_y;
        if y_rel < 0 {
            return Ok(Err("below the world".to_string()));
        }
        let rel_x = (pos.x.rem_euclid(16) as usize >> 2) & 3;
        let rel_y = y_rel as usize >> 2;
        let rel_z = (pos.z.rem_euclid(16) as usize >> 2) & 3;
        chunk
            .section
            .set_relative_biome(rel_x, rel_y, rel_z, biome_id);
        chunk.mark_dirty(true);
        Ok(Ok(()))
    }

    fn get_entities(
        &mut self,
        world: Resource<World>,
    ) -> wasmtime::Result<Vec<Resource<pumpkin::plugin::entity::Entity>>> {
        let world_provider = self.get(&world)?.clone();
        let mut entities = Vec::new();

        // Add players as entities
        for player in world_provider.players.load().iter() {
            entities.push(self.add(player.clone() as Arc<dyn pumpkin_core::entity::EntityBase>)?);
        }

        // Add other entities
        for entity in world_provider.entities.load().iter() {
            entities.push(self.add(entity.clone())?);
        }

        Ok(entities)
    }

    fn get_entity_by_id(
        &mut self,
        world: Resource<World>,
        id: i32,
    ) -> wasmtime::Result<Option<Resource<pumpkin::plugin::entity::Entity>>> {
        let world_provider = self.get(&world)?.clone();
        if let Some(entity) = world_provider.get_entity_by_id(id) {
            Ok(Some(self.add(entity)?))
        } else {
            Ok(None)
        }
    }

    fn get_entity_by_uuid(
        &mut self,
        world: Resource<World>,
        id: crate::pumpkin::plugin::uuid::Uuid,
    ) -> wasmtime::Result<Option<Resource<pumpkin::plugin::entity::Entity>>> {
        let world_provider = self.get(&world)?.clone();
        let uuid = <crate::pumpkin::plugin::uuid::Uuid as crate::uuid::UuidExt>::from_wit(&id);
        if let Some(player) = world_provider.get_player_by_uuid(uuid) {
            return Ok(Some(
                self.add(player as Arc<dyn pumpkin_core::entity::EntityBase>)?,
            ));
        }
        if let Some(entity) = world_provider.get_entity_by_uuid(uuid) {
            return Ok(Some(self.add(entity)?));
        }
        Ok(None)
    }

    fn get_entities_in_box(
        &mut self,
        world: Resource<World>,
        min: WitPosition,
        max: WitPosition,
    ) -> wasmtime::Result<Vec<Resource<pumpkin::plugin::entity::Entity>>> {
        let world_provider = self.get(&world)?.clone();
        let min_vec = super::events::from_wasm_position(min);
        let max_vec = super::events::from_wasm_position(max);
        let aabb = pumpkin_util::math::boundingbox::BoundingBox::new(
            pumpkin_util::math::vector3::Vector3::new(
                min_vec.x.min(max_vec.x),
                min_vec.y.min(max_vec.y),
                min_vec.z.min(max_vec.z),
            ),
            pumpkin_util::math::vector3::Vector3::new(
                min_vec.x.max(max_vec.x),
                min_vec.y.max(max_vec.y),
                min_vec.z.max(max_vec.z),
            ),
        );
        let found = world_provider.get_all_at_box(&aabb);
        let mut results = Vec::with_capacity(found.len());
        for entity in found {
            results.push(self.add(entity)?);
        }
        Ok(results)
    }

    fn ray_trace_blocks(
        &mut self,
        world: Resource<World>,
        start: WitPosition,
        end: WitPosition,
    ) -> wasmtime::Result<Option<WitPosition>> {
        let world_provider = self.get(&world)?.clone();
        let start_pos = super::events::from_wasm_position(start);
        let end_pos = super::events::from_wasm_position(end);
        let res = world_provider.raycast(start_pos, end_pos, |pos, w| {
            !w.get_block_state(pos).is_air()
        });
        Ok(res.map(|(p, _)| {
            super::events::to_wasm_position(pumpkin_util::math::vector3::Vector3::new(
                f64::from(p.0.x),
                f64::from(p.0.y),
                f64::from(p.0.z),
            ))
        }))
    }

    fn ray_trace_block(
        &mut self,
        world: Resource<World>,
        start: WitPosition,
        end: WitPosition,
        include_fluids: bool,
    ) -> wasmtime::Result<Option<WitRayTraceBlockResult>> {
        let world_provider = self.get(&world)?.clone();
        let start_pos = super::events::from_wasm_position(start);
        let end_pos = super::events::from_wasm_position(end);
        let res = world_provider.ray_trace_block(start_pos, end_pos, include_fluids);
        Ok(res.map(|(pos, face, hit_pos)| WitRayTraceBlockResult {
            pos: WitBlockPos {
                x: pos.0.x,
                y: pos.0.y,
                z: pos.0.z,
            },
            face: to_wasm_block_direction(face),
            hit_pos: super::events::to_wasm_position(hit_pos),
        }))
    }

    fn ray_trace_entity(
        &mut self,
        world: Resource<World>,
        start: WitPosition,
        end: WitPosition,
    ) -> wasmtime::Result<Option<WitRayTraceEntityResult>> {
        let world_provider = self.get(&world)?.clone();
        let start_pos = super::events::from_wasm_position(start);
        let end_pos = super::events::from_wasm_position(end);
        if let Some((entity, hit_pos, distance)) =
            world_provider.ray_trace_entity(start_pos, end_pos)
        {
            let entity_res = self
                .add(entity)
                .map_err(|_| wasmtime::Error::msg("failed to add entity resource"))?;
            Ok(Some(WitRayTraceEntityResult {
                entity: entity_res,
                hit_pos: super::events::to_wasm_position(hit_pos),
                distance,
            }))
        } else {
            Ok(None)
        }
    }

    fn ray_trace_entities(
        &mut self,
        world: Resource<World>,
        start: WitPosition,
        end: WitPosition,
    ) -> wasmtime::Result<Vec<WitRayTraceEntityResult>> {
        let world_provider = self.get(&world)?.clone();
        let start_pos = super::events::from_wasm_position(start);
        let end_pos = super::events::from_wasm_position(end);
        let hits = world_provider.ray_trace_entities(start_pos, end_pos);
        let mut results = Vec::with_capacity(hits.len());
        for (entity, hit_pos, distance) in hits {
            let entity_res = self
                .add(entity)
                .map_err(|_| wasmtime::Error::msg("failed to add entity resource"))?;
            results.push(WitRayTraceEntityResult {
                entity: entity_res,
                hit_pos: super::events::to_wasm_position(hit_pos),
                distance,
            });
        }
        Ok(results)
    }

    fn get_block_entity(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<Option<BlockEntityType>> {
        let world_provider = self.get(&world)?.clone();
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let block_entity = world_provider.get_block_entity(&internal_pos);

        block_entity.map_or_else(|| Ok(None), |be| self.get_wit_block_entity_v0_2(be))
    }

    fn get_block_entity_nbt(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        let world_ref = self.get(&world)?.clone();
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);

        let Some(entity) = world_ref.get_block_entity(&internal_pos) else {
            return Ok(None);
        };

        let mut nbt = pumpkin_nbt::NbtCompound::new();
        entity.write_internal(&mut nbt);

        let bytes = pumpkin_nbt::Nbt::from(nbt).write_unnamed();
        Ok(Some(bytes.to_vec()))
    }

    fn set_block_entity_nbt(
        &mut self,
        world: Resource<World>,
        pos: WitBlockPos,
        nbt_data: Vec<u8>,
    ) -> wasmtime::Result<Result<(), String>> {
        let world_ref = self.get(&world)?.clone();
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);

        let mut cursor = std::io::Cursor::new(&nbt_data[..]);
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
            pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
        );
        let mut nbt: pumpkin_nbt::NbtCompound = pumpkin_nbt::Nbt::read_unnamed(&mut reader)
            .map_err(|e| wasmtime::Error::msg(format!("Invalid NBT: {e}")))?
            .root_tag;

        // Override NBT position with the caller-provided position so tile
        // entities land at the correct coordinates during schematic pastes.
        nbt.put_int("x", pos.x);
        nbt.put_int("y", pos.y);
        nbt.put_int("z", pos.z);

        // Use add_block_entity_nbt for lazy loading — avoids broadcasting
        // a packet per tile entity during bulk operations.
        world_ref.add_block_entity_nbt(internal_pos, &nbt);
        Ok(Ok(()))
    }

    fn set_chunk_generator(
        &mut self,
        world: Resource<World>,
        generator_id: u32,
    ) -> wasmtime::Result<()> {
        let world_ref = self.get(&world)?.clone();
        let Some(plugin_weak) = self.plugin.as_ref() else {
            return Ok(());
        };
        let Some(plugin) = plugin_weak.upgrade() else {
            return Ok(());
        };
        let Some(server) = self.server.clone() else {
            return Ok(());
        };

        let wasm_gen = Arc::new(WasmChunkGenerator::new(
            generator_id,
            plugin,
            world_ref.dimension.clone(),
            world_ref.level.seed.0,
            server,
        ));

        world_ref.level.set_world_gen(Arc::new(
            pumpkin_world::generation::generator::WorldGenerator::Custom(wasm_gen),
        ));
        Ok(())
    }

    fn add_chunk_populator(
        &mut self,
        world: Resource<World>,
        generator_id: u32,
    ) -> wasmtime::Result<()> {
        add_populator(
            self,
            &world,
            generator_id,
            pumpkin_world::generation::populator::PopulatorStage::Features,
        )
    }

    fn add_surface_populator(
        &mut self,
        world: Resource<World>,
        generator_id: u32,
    ) -> wasmtime::Result<()> {
        add_populator(
            self,
            &world,
            generator_id,
            pumpkin_world::generation::populator::PopulatorStage::Surface,
        )
    }

    fn get_name(&mut self, world: Resource<World>) -> wasmtime::Result<String> {
        Ok(self.get(&world)?.get_world_name().to_string())
    }

    fn set_custom_data(
        &mut self,
        world: Resource<World>,
        namespace: String,
        key: String,
        value: super::common::WitNbtTree,
    ) -> wasmtime::Result<()> {
        let world_res = self.get(&world)?;
        let tag = super::common::from_wit_nbt_tree(&value).map_err(wasmtime::Error::msg)?;
        world_res.set_custom_data(&namespace, &key, tag);
        Ok(())
    }

    fn get_custom_data(
        &mut self,
        world: Resource<World>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<Option<super::common::WitNbtTree>> {
        let world_res = self.get(&world)?;
        let tag = world_res.get_custom_data(&namespace, &key);
        Ok(tag.map(super::common::to_wit_nbt_tree))
    }

    fn remove_custom_data(
        &mut self,
        world: Resource<World>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<()> {
        let world_res = self.get(&world)?;
        world_res.remove_custom_data(&namespace, &key);
        Ok(())
    }

    fn has_custom_data(
        &mut self,
        world: Resource<World>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<bool> {
        let world_res = self.get(&world)?;
        Ok(world_res.has_custom_data(&namespace, &key))
    }

    fn get_game_rule(
        &mut self,
        world: Resource<World>,
        rule: WitGameRule,
    ) -> wasmtime::Result<WitGameRuleValue> {
        let world_res = self.get(&world)?;
        let internal_rule = from_wit_game_rule(rule);
        let value = world_res.get_game_rule(&internal_rule);
        Ok(to_wit_game_rule_value(&value))
    }

    fn set_game_rule(
        &mut self,
        world: Resource<World>,
        rule: WitGameRule,
        value: WitGameRuleValue,
    ) -> wasmtime::Result<()> {
        let world_res = self.get(&world)?;
        let internal_rule = from_wit_game_rule(rule);
        let internal_value = from_wit_game_rule_value(value);
        world_res.set_game_rule(&internal_rule, internal_value);
        Ok(())
    }

    fn drop(&mut self, rep: Resource<World>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

impl pumpkin::plugin::world::HostWorldWithStore<PluginHostState> for HasSelf<PluginHostState> {
    async fn set_raining(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        raining: bool,
    ) -> wasmtime::Result<()> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        run_blocking(accessor, move || world.set_raining(raining)).await
    }

    async fn set_thundering(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        thundering: bool,
    ) -> wasmtime::Result<()> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        run_blocking(accessor, move || world.set_thundering(thundering)).await
    }

    async fn create_explosion(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: pumpkin::plugin::common::Position,
        power: f32,
        _create_fire: bool,
        interaction: pumpkin::plugin::world::ExplosionInteraction,
    ) -> wasmtime::Result<()> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let interaction = match interaction {
            pumpkin::plugin::world::ExplosionInteraction::None => ExplosionInteraction::None,
            pumpkin::plugin::world::ExplosionInteraction::Block => ExplosionInteraction::Block,
            pumpkin::plugin::world::ExplosionInteraction::Mob => ExplosionInteraction::Mob,
            pumpkin::plugin::world::ExplosionInteraction::Tnt => ExplosionInteraction::Tnt,
            pumpkin::plugin::world::ExplosionInteraction::Trigger => ExplosionInteraction::Trigger,
        };
        let pos = pumpkin_util::math::vector3::Vector3::new(pos.0, pos.1, pos.2);
        run_blocking(accessor, move || world.explode(pos, power, interaction)).await
    }

    async fn spawn_entity(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        entity_type: pumpkin::plugin::entity_types::EntityType,
        pos: pumpkin::plugin::common::Position,
    ) -> wasmtime::Result<Resource<pumpkin::plugin::world::Entity>> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;

        let internal_type = super::entity::from_wit_entity_type(entity_type)?;
        let pos = pumpkin_util::math::vector3::Vector3::new(pos.0, pos.1, pos.2);
        let entity = pumpkin_core::entity::r#type::from_type(
            internal_type,
            pos,
            &world,
            uuid::Uuid::new_v4(),
        );
        let spawned_entity = Arc::clone(&entity);
        run_blocking(accessor, move || world.spawn_entity(spawned_entity)).await?;
        accessor.add_res(entity)
    }

    async fn strike_lightning(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitPosition,
        effect_only: bool,
    ) -> wasmtime::Result<()> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let pos = super::events::from_wasm_position(pos);
        run_blocking(accessor, move || world.strike_lightning(pos, effect_only)).await
    }

    async fn save(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
    ) -> wasmtime::Result<Result<(), String>> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        in_active_context(accessor, world.save()).await?;
        Ok(Ok(()))
    }

    async fn set_block(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        block: WitBlock,
        update_flags: WitBlockFlags,
    ) -> wasmtime::Result<()> {
        let block_id = BlockId::new_or_air(block.id);
        let default_state_id = pumpkin_data::Block::from_id(block_id)
            .default_state
            .id
            .as_u16();
        set_block_state_with_store(accessor, world, pos, default_state_id, update_flags).await
    }

    async fn set_block_by_id(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        block_id: u16,
        update_flags: WitBlockFlags,
    ) -> wasmtime::Result<()> {
        let Some(id) = BlockId::new(block_id) else {
            return Err(wasmtime::Error::msg("Invalid BlockId"));
        };
        let default_state_id = pumpkin_data::Block::from_id(id).default_state.id.as_u16();
        set_block_state_with_store(accessor, world, pos, default_state_id, update_flags).await
    }

    async fn set_block_by_name(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        name: String,
        update_flags: WitBlockFlags,
    ) -> wasmtime::Result<bool> {
        let Some(block) = pumpkin_data::Block::from_name(&name) else {
            return Ok(false);
        };
        let default_state_id = block.default_state.id.as_u16();
        set_block_state_with_store(accessor, world, pos, default_state_id, update_flags).await?;
        Ok(true)
    }

    async fn set_block_state(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        state: u16,
        update_flags: WitBlockFlags,
    ) -> wasmtime::Result<()> {
        set_block_state_with_store(accessor, world, pos, state, update_flags).await
    }

    async fn place_structure_template(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        name: String,
        pos: WitBlockPos,
        rotation: pumpkin::plugin::world::StructureRotation,
        skip_air: bool,
    ) -> wasmtime::Result<Result<(), String>> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let rotation = from_wit_structure_rotation(rotation);
        run_blocking(accessor, move || {
            pumpkin_core::plugin::api::structure_template::place(
                &world,
                &name,
                pumpkin_util::math::vector3::Vector3::new(pos.x, pos.y, pos.z),
                rotation,
                skip_air,
            )
        })
        .await
    }

    async fn is_chunk_generated(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<bool> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let pos = pumpkin_util::math::vector2::Vector2::new(x, z);
        Ok(world.level.is_chunk_generated(pos).await)
    }

    async fn load_chunk(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        x: i32,
        z: i32,
        generate: bool,
    ) -> wasmtime::Result<Option<Resource<WitChunk>>> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let pos = pumpkin_util::math::vector2::Vector2::new(x, z);
        if !generate && !world.level.is_chunk_generated(pos).await {
            return Ok(None);
        }
        let chunk = world.level.get_or_fetch_chunk(pos, Arc::clone).await;
        let res = accessor.add_res((world, Arc::downgrade(&chunk)))?;
        Ok(Some(res))
    }

    async fn get_chunk_at_async(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<Resource<WitChunk>> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let pos = pumpkin_util::math::vector2::Vector2::new(x, z);
        let chunk = world.level.get_or_fetch_chunk(pos, Arc::clone).await;
        accessor.add_res((world, Arc::downgrade(&chunk)))
    }

    async fn delete_chunk(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<bool> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let pos = pumpkin_util::math::vector2::Vector2::new(x, z);
        Ok(world.level.delete_chunk(pos).await)
    }

    async fn break_block(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        drop_items: bool,
        update_flags: WitBlockFlags,
    ) -> wasmtime::Result<bool> {
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let mut flags = from_wit_block_flags(update_flags);
        if !drop_items {
            flags.insert(pumpkin_world::world::BlockFlags::SKIP_DROPS);
        }
        run_blocking(accessor, move || {
            world.break_block(&internal_pos, None, flags).is_some()
        })
        .await
    }

    async fn break_block_as(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        cause: Option<Resource<pumpkin::plugin::world::Entity>>,
        update_flags: WitBlockFlags,
    ) -> wasmtime::Result<bool> {
        let (world, cause_entity) = accessor.with(|mut host| -> wasmtime::Result<_> {
            let state = host.get();
            let world = world_from_state(state, &world)?;
            let entity = cause.map(|c| state.get(&c).cloned()).transpose()?;
            Ok((world, entity))
        })?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let flags = from_wit_block_flags(update_flags);
        let player =
            cause_entity.and_then(|e| world.get_player_by_uuid(e.get_entity().entity_uuid));
        run_blocking(accessor, move || {
            world
                .break_block(&internal_pos, player.as_ref(), flags)
                .is_some()
        })
        .await
    }

    async fn break_block_naturally(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        tool: Option<Resource<crate::pumpkin::plugin::item_stack::ItemStack>>,
        trigger_effect: bool,
        update_flags: WitBlockFlags,
    ) -> wasmtime::Result<bool> {
        let tool_stack = if let Some(tool_res) = tool {
            Some(accessor.take_res(tool_res)?.lock().await.clone())
        } else {
            None
        };
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let flags = from_wit_block_flags(update_flags);
        run_blocking(accessor, move || {
            world.break_block_naturally(&internal_pos, tool_stack, trigger_effect, flags)
        })
        .await
    }

    async fn get_block_drops(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        tool: Option<Resource<crate::pumpkin::plugin::item_stack::ItemStack>>,
        entity: Option<Resource<pumpkin::plugin::world::Entity>>,
    ) -> wasmtime::Result<Vec<Resource<crate::pumpkin::plugin::item_stack::ItemStack>>> {
        let tool_stack = if let Some(tool_res) = tool {
            Some(accessor.take_res(tool_res)?.lock().await.clone())
        } else {
            None
        };
        let (world, entity_ref) = accessor.with(|mut host| -> wasmtime::Result<_> {
            let state = host.get();
            let world = world_from_state(state, &world)?;
            let ent = entity.map(|e| state.get(&e).cloned()).transpose()?;
            Ok((world, ent))
        })?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let drops = run_blocking(accessor, move || {
            world.get_block_drops(&internal_pos, tool_stack, entity_ref.as_deref())
        })
        .await?;
        let mut results = Vec::with_capacity(drops.len());
        for stack in drops {
            results.push(accessor.add_res(Arc::new(tokio::sync::Mutex::new(stack)))?);
        }
        Ok(results)
    }

    async fn is_preferred_tool(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        tool: Option<Resource<crate::pumpkin::plugin::item_stack::ItemStack>>,
    ) -> wasmtime::Result<bool> {
        let tool_stack = if let Some(tool_res) = tool {
            Some(accessor.take_res(tool_res)?.lock().await.clone())
        } else {
            None
        };
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        let (block, state) = world.get_block_and_state(&internal_pos);
        Ok(!state.tool_required()
            || tool_stack.is_some_and(|stack| stack.is_correct_for_drops(block)))
    }

    async fn drop_item(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitPosition,
        item: Resource<crate::pumpkin::plugin::item_stack::ItemStack>,
    ) -> wasmtime::Result<Resource<pumpkin::plugin::world::Entity>> {
        let stack = accessor.take_res(item)?.lock().await.clone();
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let pos_vec = super::events::from_wasm_position(pos);
        let entity = pumpkin_core::entity::Entity::new(
            Arc::clone(&world),
            pos_vec,
            &pumpkin_data::entity::EntityType::ITEM,
        );
        let item_entity: Arc<dyn pumpkin_core::entity::EntityBase> =
            Arc::new(pumpkin_core::entity::item::ItemEntity::new_with_velocity(
                entity,
                stack,
                pumpkin_util::math::vector3::Vector3::default(),
                pumpkin_core::entity::item::ItemEntity::DEFAULT_PICKUP_DELAY,
            ));
        let spawned = Arc::clone(&item_entity);
        run_blocking(accessor, move || {
            world.spawn_entity(spawned);
        })
        .await?;
        accessor.add_res(item_entity)
    }

    async fn drop_item_naturally(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitPosition,
        item: Resource<crate::pumpkin::plugin::item_stack::ItemStack>,
    ) -> wasmtime::Result<Resource<pumpkin::plugin::world::Entity>> {
        let stack = accessor.take_res(item)?.lock().await.clone();
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let pos_vec = super::events::from_wasm_position(pos);
        let height = f64::from(pumpkin_data::entity::EntityType::ITEM.dimension[1]) / 2.0;
        let natural_pos = pumpkin_util::math::vector3::Vector3::new(
            pos_vec.x + 0.5 + rand::random_range(-0.25..0.25),
            pos_vec.y + 0.5 + rand::random_range(-0.25..0.25) - height,
            pos_vec.z + 0.5 + rand::random_range(-0.25..0.25),
        );
        let entity = pumpkin_core::entity::Entity::new(
            Arc::clone(&world),
            natural_pos,
            &pumpkin_data::entity::EntityType::ITEM,
        );
        let item_entity: Arc<dyn pumpkin_core::entity::EntityBase> =
            Arc::new(pumpkin_core::entity::item::ItemEntity::new(entity, stack));
        let spawned = Arc::clone(&item_entity);
        run_blocking(accessor, move || {
            world.spawn_entity(spawned);
        })
        .await?;
        accessor.add_res(item_entity)
    }

    async fn drop_stack(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitBlockPos,
        item: Resource<crate::pumpkin::plugin::item_stack::ItemStack>,
    ) -> wasmtime::Result<()> {
        let stack = accessor.take_res(item)?.lock().await.clone();
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let internal_pos = BlockPos::new(pos.x, pos.y, pos.z);
        run_blocking(accessor, move || {
            world.drop_stack(&internal_pos, stack);
        })
        .await
    }

    async fn spawn_experience_orb(
        accessor: &Accessor<PluginHostState, Self>,
        world: Resource<World>,
        pos: WitPosition,
        amount: u32,
    ) -> wasmtime::Result<()> {
        if amount == 0 {
            return Ok(());
        }
        let world = accessor.with(|mut host| world_from_state(host.get(), &world))?;
        let pos_vec = super::events::from_wasm_position(pos);
        run_blocking(accessor, move || {
            pumpkin_core::entity::experience_orb::ExperienceOrbEntity::spawn(
                &world, pos_vec, amount,
            );
        })
        .await
    }
}

impl pumpkin::plugin::world::HostChunk for PluginHostState {
    fn get_x(&mut self, chunk: Resource<WitChunk>) -> wasmtime::Result<i32> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        Ok(chunk_data.x)
    }

    fn get_z(&mut self, chunk: Resource<WitChunk>) -> wasmtime::Result<i32> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        Ok(chunk_data.z)
    }

    fn get_world(&mut self, chunk: Resource<WitChunk>) -> wasmtime::Result<Resource<World>> {
        let (world, _) = self.get(&chunk)?.clone();
        self.add(world)
    }

    fn is_loaded(&mut self, chunk: Resource<WitChunk>) -> wasmtime::Result<bool> {
        let (_, chunk_data) = self.get(&chunk)?;
        Ok(chunk_data.upgrade().is_some())
    }

    fn is_force_loaded(&mut self, chunk: Resource<WitChunk>) -> wasmtime::Result<bool> {
        let (world, chunk_data) = self.get(&chunk)?;
        let Some(chunk_data) = chunk_data.upgrade() else {
            return Ok(false);
        };
        let pos = pumpkin_util::math::vector2::Vector2::new(chunk_data.x, chunk_data.z);
        Ok(world.forced_chunks.lock_ignore_poison().contains(&pos))
    }

    fn set_force_loaded(
        &mut self,
        chunk: Resource<WitChunk>,
        forced: bool,
    ) -> wasmtime::Result<()> {
        let (world, chunk_data) = self.get(&chunk)?.clone();
        let Some(chunk_data) = chunk_data.upgrade() else {
            return Err(wasmtime::Error::msg("Chunk unloaded"));
        };
        let pos = pumpkin_util::math::vector2::Vector2::new(chunk_data.x, chunk_data.z);
        let changed = {
            let mut forced_chunks = world.forced_chunks.lock_ignore_poison();
            if forced {
                forced_chunks.insert(pos)
            } else {
                forced_chunks.remove(&pos)
            }
        };
        if changed {
            let mut loading = world.level.chunk_loading.lock_ignore_poison();
            if forced {
                loading.add_force_ticket(pos);
            } else {
                loading.remove_force_ticket(pos);
            }
            loading.send_change();
            drop(loading);
            world.update_active_chunks();
        }
        Ok(())
    }

    fn get_entities(
        &mut self,
        chunk: Resource<WitChunk>,
    ) -> wasmtime::Result<Vec<Resource<pumpkin::plugin::entity::Entity>>> {
        let (world, chunk_data) = self.get(&chunk)?.clone();
        let Some(chunk_data) = chunk_data.upgrade() else {
            return Ok(Vec::new());
        };
        let chunk_pos = pumpkin_util::math::vector2::Vector2::new(chunk_data.x, chunk_data.z);
        let mut entities = Vec::new();
        for player in world.players.load().iter() {
            if player.living_entity.entity.chunk_pos.load() == chunk_pos {
                entities
                    .push(self.add(player.clone() as Arc<dyn pumpkin_core::entity::EntityBase>)?);
            }
        }
        for entity in world.entities.load().iter() {
            if entity.get_entity().chunk_pos.load() == chunk_pos {
                entities.push(self.add(entity.clone())?);
            }
        }
        Ok(entities)
    }

    fn get_inhabited_time(&mut self, chunk: Resource<WitChunk>) -> wasmtime::Result<i64> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        Ok(chunk_data
            .inhabited_time
            .load(std::sync::atomic::Ordering::Relaxed) as i64)
    }

    fn set_inhabited_time(&mut self, chunk: Resource<WitChunk>, time: i64) -> wasmtime::Result<()> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        chunk_data
            .inhabited_time
            .store(time.max(0) as u64, std::sync::atomic::Ordering::Relaxed);
        chunk_data.mark_dirty(true);
        Ok(())
    }

    fn get_block_state_id(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<u16> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        Ok(chunk_data
            .section
            .get_block_absolute_y(pos.x as usize, pos.y, pos.z as usize)
            .unwrap_or(BlockStateId::AIR)
            .as_u16())
    }

    fn get_block_state(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<WitBlockState> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        let id = chunk_data
            .section
            .get_block_absolute_y(pos.x as usize, pos.y, pos.z as usize)
            .unwrap_or(BlockStateId::AIR);
        let state = id.to_state();
        let world_pos = BlockPos::new(chunk_data.x * 16 + pos.x, pos.y, chunk_data.z * 16 + pos.z);
        Ok(to_wit_block_state(state, Some(&world_pos)))
    }

    fn get_block(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<WitBlock> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        let id = chunk_data
            .section
            .get_block_absolute_y(pos.x as usize, pos.y, pos.z as usize)
            .unwrap_or(BlockStateId::AIR);
        let block = pumpkin_data::Block::from_state_id(id);
        Ok(to_wit_block(block))
    }

    fn set_block(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
        block: WitBlock,
    ) -> wasmtime::Result<()> {
        let block_id = BlockId::new_or_air(block.id);
        let default_state_id = pumpkin_data::Block::from_id(block_id)
            .default_state
            .id
            .as_u16();
        self.set_block_state(chunk, pos, default_state_id)
    }

    fn set_block_by_id(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
        block_id: u16,
    ) -> wasmtime::Result<()> {
        let Some(id) = BlockId::new(block_id) else {
            return Err(wasmtime::Error::msg("Invalid BlockId"));
        };
        let default_state_id = pumpkin_data::Block::from_id(id).default_state.id.as_u16();
        self.set_block_state(chunk, pos, default_state_id)
    }

    fn set_block_state(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
        state: u16,
    ) -> wasmtime::Result<()> {
        let chunk_res = self.get(&chunk)?;
        let (world, chunk_data) = &chunk_res;
        let Some(chunk_data) = chunk_data.upgrade() else {
            return Err(wasmtime::Error::msg("Chunk unloaded"));
        };

        let Some(state) = BlockStateId::new(state) else {
            return Err(wasmtime::Error::msg("Invalid BlockStateId"));
        };

        let replaced =
            chunk_data.set_block_absolute_y(pos.x as usize, pos.y, pos.z as usize, state);

        if replaced != state {
            chunk_data.mark_dirty(true);
            let absolute_pos =
                BlockPos::new(chunk_data.x * 16 + pos.x, pos.y, chunk_data.z * 16 + pos.z);
            world.register_block_change(absolute_pos, state);
        }

        Ok(())
    }

    fn get_biome(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<pumpkin::plugin::biomes::Biome> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        let id = chunk_data
            .section
            .get_rough_biome_absolute_y(pos.x as usize, pos.y, pos.z as usize)
            .unwrap_or(0);
        let biome = pumpkin_data::biome::Biome::from_id(id)
            .unwrap_or(&pumpkin_data::biome::Biome::THE_VOID);

        Self::get_wit_biome_v0_2(biome)
    }

    fn set_biome(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
        biome: pumpkin::plugin::biomes::Biome,
    ) -> wasmtime::Result<()> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        let internal_biome = from_wit_biome(biome)?;
        let y_rel = pos.y - chunk_data.section.min_y;
        if y_rel >= 0 && (0..16).contains(&pos.x) && (0..16).contains(&pos.z) {
            let rel_x = (pos.x as usize >> 2) & 3;
            let rel_y = y_rel as usize >> 2;
            let rel_z = (pos.z as usize >> 2) & 3;
            chunk_data
                .section
                .set_relative_biome(rel_x, rel_y, rel_z, internal_biome.id);
            chunk_data.mark_dirty(true);
        }
        Ok(())
    }

    fn get_block_entity(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<Option<BlockEntityType>> {
        let chunk_res = self.get(&chunk)?;
        let (world, chunk_data) = &chunk_res;
        let Some(chunk_data) = chunk_data.upgrade() else {
            return Err(wasmtime::Error::msg("Chunk unloaded"));
        };
        let absolute_pos =
            BlockPos::new(chunk_data.x * 16 + pos.x, pos.y, chunk_data.z * 16 + pos.z);
        let block_entity = world.get_block_entity(&absolute_pos);

        block_entity.map_or_else(|| Ok(None), |be| self.get_wit_block_entity_v0_2(be))
    }

    fn get_top_block_y(
        &mut self,
        chunk: Resource<WitChunk>,
        x: i32,
        z: i32,
    ) -> wasmtime::Result<i32> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        Ok(chunk_data.heightmap.lock_ignore_poison().get(
            ChunkHeightmapType::WorldSurface,
            x,
            z,
            chunk_data.section.min_y,
        ))
    }

    fn get_sky_light(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<u8> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        let section_index = (pos.y - chunk_data.section.min_y) as usize / 16;
        Ok(chunk_data
            .light_engine
            .lock_ignore_poison()
            .sky_light
            .get(section_index)
            .map_or(0, |c| {
                c.get(pos.x as usize, pos.y as usize % 16, pos.z as usize)
            }))
    }

    fn get_block_light(
        &mut self,
        chunk: Resource<WitChunk>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<u8> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        let section_index = (pos.y - chunk_data.section.min_y) as usize / 16;
        Ok(chunk_data
            .light_engine
            .lock_ignore_poison()
            .block_light
            .get(section_index)
            .map_or(0, |c| {
                c.get(pos.x as usize, pos.y as usize % 16, pos.z as usize)
            }))
    }

    fn set_custom_data(
        &mut self,
        chunk: Resource<WitChunk>,
        namespace: String,
        key: String,
        value: super::common::WitNbtTree,
    ) -> wasmtime::Result<()> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        let tag = super::common::from_wit_nbt_tree(&value).map_err(wasmtime::Error::msg)?;
        chunk_data.set_custom_data(&namespace, &key, tag);
        Ok(())
    }

    fn get_custom_data(
        &mut self,
        chunk: Resource<WitChunk>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<Option<super::common::WitNbtTree>> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        let tag = chunk_data.get_custom_data(&namespace, &key);
        Ok(tag.map(super::common::to_wit_nbt_tree))
    }

    fn remove_custom_data(
        &mut self,
        chunk: Resource<WitChunk>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<()> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        chunk_data.remove_custom_data(&namespace, &key);
        Ok(())
    }

    fn has_custom_data(
        &mut self,
        chunk: Resource<WitChunk>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<bool> {
        let chunk_data = loaded_chunk_data(self, &chunk)?;
        Ok(chunk_data.has_custom_data(&namespace, &key))
    }

    fn drop(&mut self, rep: Resource<WitChunk>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

impl pumpkin::plugin::world::HostWorldBorder for PluginHostState {
    fn get_center_x(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<f64> {
        let border_res = self.get(&border)?;
        Ok(border_res.worldborder.lock_ignore_poison().center_x)
    }

    fn get_center_z(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<f64> {
        let border_res = self.get(&border)?;
        Ok(border_res.worldborder.lock_ignore_poison().center_z)
    }

    fn get_center(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<WitPosition> {
        let border_res = self.get(&border)?;
        let guard = border_res.worldborder.lock_ignore_poison();
        Ok(super::events::to_wasm_position(
            pumpkin_util::math::vector3::Vector3::new(guard.center_x, 0.0, guard.center_z),
        ))
    }

    fn set_center(
        &mut self,
        border: Resource<WitWorldBorder>,
        x: f64,
        z: f64,
    ) -> wasmtime::Result<()> {
        let border_res = self.get(&border)?;
        let world = border_res.clone();
        world
            .worldborder
            .lock_ignore_poison()
            .set_center(&world, x, z);
        Ok(())
    }

    fn get_diameter(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<f64> {
        let border_res = self.get(&border)?;
        Ok(border_res.worldborder.lock_ignore_poison().new_diameter)
    }

    fn get_size(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<f64> {
        self.get_diameter(border)
    }

    fn set_diameter(
        &mut self,
        border: Resource<WitWorldBorder>,
        diameter: f64,
        speed: Option<u64>,
    ) -> wasmtime::Result<()> {
        let border_res = self.get(&border)?;
        let world = border_res.clone();
        world.worldborder.lock_ignore_poison().set_diameter(
            &world,
            diameter,
            speed.map(|s| s as i64),
        );
        Ok(())
    }

    fn set_size(&mut self, border: Resource<WitWorldBorder>, size: f64) -> wasmtime::Result<()> {
        self.set_diameter(border, size, None)
    }

    fn set_size_transition(
        &mut self,
        border: Resource<WitWorldBorder>,
        new_size: f64,
        time_seconds: u64,
    ) -> wasmtime::Result<()> {
        let speed_millis = time_seconds.saturating_mul(1000);
        self.set_diameter(border, new_size, Some(speed_millis))
    }

    fn get_target_diameter(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<f64> {
        let border_res = self.get(&border)?;
        Ok(border_res.worldborder.lock_ignore_poison().new_diameter)
    }

    fn get_target_speed(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<i64> {
        let border_res = self.get(&border)?;
        Ok(border_res.worldborder.lock_ignore_poison().speed)
    }

    fn get_warning_distance(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<i32> {
        let border_res = self.get(&border)?;
        Ok(border_res.worldborder.lock_ignore_poison().warning_blocks)
    }

    fn set_warning_distance(
        &mut self,
        border: Resource<WitWorldBorder>,
        distance: i32,
    ) -> wasmtime::Result<()> {
        let border_res = self.get(&border)?;
        let world = border_res.clone();
        world
            .worldborder
            .lock_ignore_poison()
            .set_warning_distance(&world, distance);
        Ok(())
    }

    fn get_warning_delay(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<i32> {
        let border_res = self.get(&border)?;
        Ok(border_res.worldborder.lock_ignore_poison().warning_time)
    }

    fn set_warning_delay(
        &mut self,
        border: Resource<WitWorldBorder>,
        delay: i32,
    ) -> wasmtime::Result<()> {
        let border_res = self.get(&border)?;
        let world = border_res.clone();
        world
            .worldborder
            .lock_ignore_poison()
            .set_warning_delay(&world, delay);
        Ok(())
    }

    fn get_warning_time(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<i32> {
        self.get_warning_delay(border)
    }

    fn set_warning_time(
        &mut self,
        border: Resource<WitWorldBorder>,
        time: i32,
    ) -> wasmtime::Result<()> {
        self.set_warning_delay(border, time)
    }

    fn get_damage_buffer(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<f64> {
        let border_res = self.get(&border)?;
        Ok(f64::from(
            border_res.worldborder.lock_ignore_poison().buffer,
        ))
    }

    fn set_damage_buffer(
        &mut self,
        border: Resource<WitWorldBorder>,
        buffer: f64,
    ) -> wasmtime::Result<()> {
        let border_res = self.get(&border)?;
        border_res
            .worldborder
            .lock_ignore_poison()
            .set_damage_buffer(buffer as f32);
        Ok(())
    }

    fn get_damage_amount(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<f64> {
        let border_res = self.get(&border)?;
        Ok(f64::from(
            border_res.worldborder.lock_ignore_poison().damage_per_block,
        ))
    }

    fn set_damage_amount(
        &mut self,
        border: Resource<WitWorldBorder>,
        damage: f64,
    ) -> wasmtime::Result<()> {
        let border_res = self.get(&border)?;
        border_res
            .worldborder
            .lock_ignore_poison()
            .set_damage_per_block(damage as f32);
        Ok(())
    }

    fn contains(
        &mut self,
        border: Resource<WitWorldBorder>,
        x: f64,
        z: f64,
    ) -> wasmtime::Result<bool> {
        let border_res = self.get(&border)?;
        Ok(border_res.worldborder.lock_ignore_poison().contains(x, z))
    }

    fn contains_pos(
        &mut self,
        border: Resource<WitWorldBorder>,
        pos: WitPosition,
    ) -> wasmtime::Result<bool> {
        let border_res = self.get(&border)?;
        Ok(border_res
            .worldborder
            .lock_ignore_poison()
            .contains(pos.0, pos.2))
    }

    fn reset(&mut self, border: Resource<WitWorldBorder>) -> wasmtime::Result<()> {
        let border_res = self.get(&border)?;
        let world = border_res.clone();
        world.worldborder.lock_ignore_poison().reset(&world);
        Ok(())
    }

    fn drop(&mut self, rep: Resource<WitWorldBorder>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

impl pumpkin::plugin::world::HostChunkBuffer for PluginHostState {
    fn get_x(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
    ) -> wasmtime::Result<i32> {
        let res = self.get(&this)?;
        Ok(res.x)
    }

    fn get_z(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
    ) -> wasmtime::Result<i32> {
        let res = self.get(&this)?;
        Ok(res.z)
    }

    fn get_min_y(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
    ) -> wasmtime::Result<i32> {
        let res = self.get(&this)?;
        Ok(res.min_y)
    }

    fn get_height(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
    ) -> wasmtime::Result<u32> {
        let res = self.get(&this)?;
        Ok(res.height)
    }

    fn set_block_state_id(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        x: u8,
        y: i32,
        z: u8,
        state_id: u16,
    ) -> wasmtime::Result<()> {
        let res = self.get(&this)?;
        if x < 16 && z < 16 {
            let world_x =
                pumpkin_world::generation::positions::chunk_pos::start_block_x(res.x) + x as i32;
            let world_z =
                pumpkin_world::generation::positions::chunk_pos::start_block_z(res.z) + z as i32;
            let mut proto = CenterGuard::lock(res);
            let block_state = pumpkin_data::BlockState::from_id(
                pumpkin_data::BlockStateId::new(state_id)
                    .unwrap_or(pumpkin_data::BlockStateId::AIR),
            );
            proto.set_block_state(world_x, y, world_z, block_state);
        }
        Ok(())
    }

    fn get_block_state_id(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        x: u8,
        y: i32,
        z: u8,
    ) -> wasmtime::Result<u16> {
        let res = self.get(&this)?;
        if x < 16 && z < 16 {
            let proto = CenterGuard::lock(res);
            let local_y = y - proto.bottom_y() as i32;
            if local_y >= 0 && local_y < proto.height() as i32 {
                Ok(proto
                    .get_block_state_raw(x as i32, local_y, z as i32)
                    .as_u16())
            } else {
                Ok(0)
            }
        } else {
            Ok(0)
        }
    }

    fn fill_layer(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        y: i32,
        state_id: u16,
    ) -> wasmtime::Result<()> {
        let res = self.get(&this)?;
        let start_x = pumpkin_world::generation::positions::chunk_pos::start_block_x(res.x);
        let start_z = pumpkin_world::generation::positions::chunk_pos::start_block_z(res.z);
        let block_state = pumpkin_data::BlockState::from_id(
            pumpkin_data::BlockStateId::new(state_id).unwrap_or(pumpkin_data::BlockStateId::AIR),
        );
        let mut proto = CenterGuard::lock(res);
        for x in 0..16 {
            for z in 0..16 {
                proto.set_block_state(start_x + x, y, start_z + z, block_state);
            }
        }
        Ok(())
    }

    fn fill_range(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        x: u8,
        min_y: i32,
        max_y: i32,
        z: u8,
        state_id: u16,
    ) -> wasmtime::Result<()> {
        let res = self.get(&this)?;
        if x < 16 && z < 16 {
            let world_x =
                pumpkin_world::generation::positions::chunk_pos::start_block_x(res.x) + x as i32;
            let world_z =
                pumpkin_world::generation::positions::chunk_pos::start_block_z(res.z) + z as i32;
            let block_state = pumpkin_data::BlockState::from_id(
                pumpkin_data::BlockStateId::new(state_id)
                    .unwrap_or(pumpkin_data::BlockStateId::AIR),
            );
            let mut proto = CenterGuard::lock(res);
            for y in min_y..=max_y {
                proto.set_block_state(world_x, y, world_z, block_state);
            }
        }
        Ok(())
    }

    fn fill_cuboid(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        min_x: u8,
        min_y: i32,
        min_z: u8,
        max_x: u8,
        max_y: i32,
        max_z: u8,
        state_id: u16,
    ) -> wasmtime::Result<()> {
        let res = self.get(&this)?;
        let start_x = pumpkin_world::generation::positions::chunk_pos::start_block_x(res.x);
        let start_z = pumpkin_world::generation::positions::chunk_pos::start_block_z(res.z);
        let block_state = pumpkin_data::BlockState::from_id(
            pumpkin_data::BlockStateId::new(state_id).unwrap_or(pumpkin_data::BlockStateId::AIR),
        );
        let mut proto = CenterGuard::lock(res);
        let max_x = max_x.min(15);
        let max_z = max_z.min(15);
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                for z in min_z..=max_z {
                    proto.set_block_state(start_x + x as i32, y, start_z + z as i32, block_state);
                }
            }
        }
        Ok(())
    }

    fn set_biome(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        x: u8,
        y: i32,
        z: u8,
        biome: pumpkin::plugin::biomes::Biome,
    ) -> wasmtime::Result<()> {
        let res = self.get(&this)?;
        if x < 16 && z < 16 {
            let biome_id = biome as u8;
            let mut proto = CenterGuard::lock(res);
            let biome_x = x as i32 / 4;
            let biome_z = z as i32 / 4;
            let biome_y = (y - proto.bottom_y() as i32) / 4;
            if biome_y >= 0 && (biome_y as usize) < (proto.height() as usize / 4) {
                let index = proto.local_biome_pos_to_biome_index(biome_x, biome_y, biome_z);
                if index < proto.flat_biome_map.len() {
                    proto.flat_biome_map[index] = biome_id;
                }
            }
        }
        Ok(())
    }

    fn fill_biome(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        biome: pumpkin::plugin::biomes::Biome,
    ) -> wasmtime::Result<()> {
        let res = self.get(&this)?;
        let biome_id = biome as u8;
        let mut proto = CenterGuard::lock(res);
        proto.flat_biome_map.fill(biome_id);
        Ok(())
    }

    fn set_biome_by_name(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        x: u8,
        y: i32,
        z: u8,
        name: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let biome_id = match biome_id_by_name(&name) {
            Ok(id) => id,
            Err(error) => return Ok(Err(error)),
        };
        let res = self.get(&this)?;
        if x < 16 && z < 16 {
            let mut proto = CenterGuard::lock(res);
            let biome_y = (y - proto.bottom_y() as i32) / 4;
            if biome_y >= 0 && (biome_y as usize) < (proto.height() as usize / 4) {
                let index =
                    proto.local_biome_pos_to_biome_index(x as i32 / 4, biome_y, z as i32 / 4);
                if index < proto.flat_biome_map.len() {
                    proto.flat_biome_map[index] = biome_id;
                }
            }
        }
        Ok(Ok(()))
    }

    fn fill_biome_by_name(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        name: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let biome_id = match biome_id_by_name(&name) {
            Ok(id) => id,
            Err(error) => return Ok(Err(error)),
        };
        let res = self.get(&this)?;
        CenterGuard::lock(res).flat_biome_map.fill(biome_id);
        Ok(Ok(()))
    }

    fn get_seed(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
    ) -> wasmtime::Result<i64> {
        Ok(self.get(&this)?.seed)
    }

    fn set_section(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        section_y: i32,
        states: Vec<u16>,
    ) -> wasmtime::Result<bool> {
        let res = self.get(&this)?;
        let base_y = section_y.saturating_mul(16);
        if states.len() != 4096 || base_y < res.min_y || base_y + 16 > res.min_y + res.height as i32
        {
            return Ok(false);
        }
        let start_x = res.x * 16;
        let start_z = res.z * 16;
        let mut proto = CenterGuard::lock(res);
        for (index, &state_id) in states.iter().enumerate() {
            let x = (index & 15) as i32;
            let z = ((index >> 4) & 15) as i32;
            let y = (index >> 8) as i32;
            proto.set_block_state(
                start_x + x,
                base_y + y,
                start_z + z,
                state_from_id(state_id),
            );
        }
        Ok(true)
    }

    fn get_section(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        section_y: i32,
    ) -> wasmtime::Result<Vec<u16>> {
        let res = self.get(&this)?;
        let base_y = section_y.saturating_mul(16);
        if base_y < res.min_y || base_y + 16 > res.min_y + res.height as i32 {
            return Ok(Vec::new());
        }
        let proto = CenterGuard::lock(res);
        let mut states = Vec::with_capacity(4096);
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    states.push(
                        proto
                            .get_block_state(&pumpkin_util::math::vector3::Vector3::new(
                                x,
                                base_y + y,
                                z,
                            ))
                            .as_u16(),
                    );
                }
            }
        }
        Ok(states)
    }

    fn set_block_relative(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        x: i32,
        y: i32,
        z: i32,
        state_id: u16,
    ) -> wasmtime::Result<bool> {
        use pumpkin_world::generation::proto_chunk::GenerationCache;
        let res = self.get(&this)?;
        let Some((world_x, world_y, world_z)) = relative_to_world(res, x, y, z) else {
            return Ok(false);
        };
        let state = state_from_id(state_id);
        let Some(region) = &res.region else {
            res.proto_chunk
                .lock_ignore_poison()
                .set_block_state(world_x, world_y, world_z, state);
            return Ok(true);
        };
        let mut cache = region.lock_ignore_poison();
        // Only chunks still being generated; finished neighbours stay as they are.
        let Some(chunk) = cache.get_chunk_mut(world_x >> 4, world_z >> 4) else {
            return Ok(false);
        };
        chunk.set_block_state(world_x, world_y, world_z, state);
        Ok(true)
    }

    fn get_block_relative(
        &mut self,
        this: Resource<pumpkin::plugin::world::ChunkBuffer>,
        x: i32,
        y: i32,
        z: i32,
    ) -> wasmtime::Result<Option<u16>> {
        use pumpkin_world::generation::proto_chunk::GenerationCache;
        let res = self.get(&this)?;
        let Some((world_x, world_y, world_z)) = relative_to_world(res, x, y, z) else {
            return Ok(None);
        };
        let pos = pumpkin_util::math::vector3::Vector3::new(world_x, world_y, world_z);
        Ok(Some(
            res.region
                .as_ref()
                .map_or_else(
                    || res.proto_chunk.lock_ignore_poison().get_block_state(&pos),
                    |region| GenerationCache::get_block_state(&*region.lock_ignore_poison(), &pos),
                )
                .as_u16(),
        ))
    }

    fn drop(&mut self, rep: Resource<pumpkin::plugin::world::ChunkBuffer>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

struct ProtoChunkRestore<'a> {
    target: &'a mut pumpkin_world::ProtoChunk,
    source: Arc<StdMutex<pumpkin_world::ProtoChunk>>,
}

impl Drop for ProtoChunkRestore<'_> {
    fn drop(&mut self) {
        let mut source = self.source.lock_ignore_poison();
        std::mem::swap(self.target, &mut source);
    }
}

/// Puts a generation cache that was lent to a plugin phase back in place, also on unwind.
struct CacheRestore<'a> {
    target: &'a mut GenerationCacheRegion,
    source: Arc<StdMutex<GenerationCacheRegion>>,
}

impl Drop for CacheRestore<'_> {
    fn drop(&mut self) {
        let mut source = self.source.lock_ignore_poison();
        std::mem::swap(self.target, &mut source);
    }
}

type GenerationCacheRegion = pumpkin_world::chunk_system::generation_cache::Cache;

/// The chunk a chunk buffer writes to: the chunk alone, or the center of a features-stage
/// region whose neighbours are reachable too.
enum CenterGuard<'a> {
    Single(std::sync::MutexGuard<'a, pumpkin_world::ProtoChunk>),
    Region(std::sync::MutexGuard<'a, GenerationCacheRegion>),
}

impl<'a> CenterGuard<'a> {
    fn lock(buffer: &'a ChunkBuffer) -> Self {
        buffer.region.as_ref().map_or_else(
            || Self::Single(buffer.proto_chunk.lock_ignore_poison()),
            |region| Self::Region(region.lock_ignore_poison()),
        )
    }
}

impl std::ops::Deref for CenterGuard<'_> {
    type Target = pumpkin_world::ProtoChunk;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Single(chunk) => chunk,
            Self::Region(cache) => {
                let mid = ((cache.size * cache.size) >> 1) as usize;
                cache.chunks[mid].get_proto_chunk()
            }
        }
    }
}

impl std::ops::DerefMut for CenterGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Single(chunk) => chunk,
            Self::Region(cache) => {
                let mid = ((cache.size * cache.size) >> 1) as usize;
                cache.chunks[mid].get_proto_chunk_mut()
            }
        }
    }
}

fn state_from_id(state_id: u16) -> &'static pumpkin_data::BlockState {
    pumpkin_data::BlockState::from_id(
        pumpkin_data::BlockStateId::new(state_id).unwrap_or(pumpkin_data::BlockStateId::AIR),
    )
}

/// The world position of a block given relative to the buffer's chunk corner, if the buffer
/// can reach it: its own chunk, or in a features region one chunk around it.
const fn relative_to_world(
    buffer: &ChunkBuffer,
    x: i32,
    y: i32,
    z: i32,
) -> Option<(i32, i32, i32)> {
    let reach = if buffer.region.is_some() { 16 } else { 0 };
    if x < -reach || x >= 16 + reach || z < -reach || z >= 16 + reach {
        return None;
    }
    if y < buffer.min_y || y >= buffer.min_y + buffer.height as i32 {
        return None;
    }
    Some((buffer.x * 16 + x, y, buffer.z * 16 + z))
}

/// Which parts of vanilla generation a plugin generator keeps.
#[derive(Clone, Copy, Default)]
pub struct VanillaParts {
    pub carvers: bool,
    pub structures: bool,
    pub features: bool,
}

impl VanillaParts {
    const fn any(self) -> bool {
        self.carvers || self.structures || self.features
    }
}

#[derive(Clone)]
pub struct WasmChunkGenerator {
    pub generator_id: u32,
    pub plugin: Arc<pumpkin_wasm_host_common::plugin::WasmPlugin>,
    pub dimension: pumpkin_data::dimension::Dimension,
    pub seed: u64,
    pub server: Arc<pumpkin_core::server::Server>,
    /// Vanilla parts kept on top of the plugin's terrain, with the generator that runs them.
    pub vanilla: Option<(
        VanillaParts,
        Arc<pumpkin_world::generation::generator::VanillaGenerator>,
    )>,
    failures: Arc<std::sync::atomic::AtomicU64>,
}

impl WasmChunkGenerator {
    #[must_use]
    pub fn new(
        generator_id: u32,
        plugin: Arc<pumpkin_wasm_host_common::plugin::WasmPlugin>,
        dimension: pumpkin_data::dimension::Dimension,
        seed: u64,
        server: Arc<pumpkin_core::server::Server>,
    ) -> Self {
        Self {
            generator_id,
            plugin,
            dimension,
            seed,
            server,
            vanilla: None,
            failures: Arc::default(),
        }
    }

    /// Keeps `parts` of vanilla generation (for this world's dimension and seed).
    #[must_use]
    pub fn with_vanilla(mut self, parts: VanillaParts) -> Self {
        if parts.any() {
            let vanilla = pumpkin_world::generation::generator::VanillaGenerator::new_with_settings(
                pumpkin_util::world_seed::Seed(self.seed),
                self.dimension.clone(),
                None,
            );
            self.vanilla = Some((parts, Arc::new(vanilla)));
        }
        self
    }

    fn kept(
        &self,
    ) -> Option<(
        VanillaParts,
        &pumpkin_world::generation::generator::VanillaGenerator,
    )> {
        self.vanilla
            .as_ref()
            .map(|(parts, generator)| (*parts, generator.as_ref()))
    }

    fn placeholder_chunk(&self, x: i32, z: i32) -> pumpkin_world::ProtoChunk {
        let placeholder_generator =
            pumpkin_world::generation::generator::WorldGenerator::Custom(Arc::new(self.clone()));
        pumpkin_world::ProtoChunk::new(x, z, &placeholder_generator)
    }

    /// Runs one phase on `proto_chunk` alone.
    fn invoke_phase(
        &self,
        phase: pumpkin::plugin::world::GenerationPhase,
        proto_chunk: &mut pumpkin_world::ProtoChunk,
    ) {
        let x = proto_chunk.x;
        let z = proto_chunk.z;
        let min_y = proto_chunk.bottom_y() as i32;
        let height = proto_chunk.height() as u32;
        let placeholder = self.placeholder_chunk(x, z);
        let owned_proto_chunk = std::mem::replace(proto_chunk, placeholder);
        let shared_proto_chunk = Arc::new(StdMutex::new(owned_proto_chunk));
        let _restore = ProtoChunkRestore {
            target: proto_chunk,
            source: Arc::clone(&shared_proto_chunk),
        };
        self.call_phase(
            phase,
            ChunkBuffer {
                x,
                z,
                min_y,
                height,
                seed: self.seed as i64,
                proto_chunk: shared_proto_chunk,
                region: None,
            },
        );
    }

    /// Runs one phase on the center chunk of a generation region; the plugin can also reach
    /// the neighbouring chunks of the region.
    fn invoke_region_phase(
        &self,
        phase: pumpkin::plugin::world::GenerationPhase,
        cache: &mut GenerationCacheRegion,
    ) {
        let mid = ((cache.size * cache.size) >> 1) as usize;
        let (x, z, min_y, height) = {
            let center = cache.chunks[mid].get_proto_chunk();
            (
                center.x,
                center.z,
                center.bottom_y() as i32,
                center.height() as u32,
            )
        };
        let placeholder = GenerationCacheRegion::new(cache.x, cache.z, cache.size);
        let owned_cache = std::mem::replace(cache, placeholder);
        let shared_cache = Arc::new(StdMutex::new(owned_cache));
        let _restore = CacheRestore {
            target: cache,
            source: Arc::clone(&shared_cache),
        };
        self.call_phase(
            phase,
            ChunkBuffer {
                x,
                z,
                min_y,
                height,
                seed: self.seed as i64,
                proto_chunk: Arc::new(StdMutex::new(self.placeholder_chunk(x, z))),
                region: Some(shared_cache),
            },
        );
    }

    /// Calls the plugin's `handle-generate-phase` and waits for it. Errors are logged and the
    /// phase is skipped, leaving the chunk as the plugin left it.
    fn call_phase(
        &self,
        phase: pumpkin::plugin::world::GenerationPhase,
        chunk_buffer: ChunkBuffer,
    ) {
        let function = {
            let plugin = self.plugin.instance::<crate::Plugin>();
            plugin.func_handle_generate_phase()
        };
        let generator_id = self.generator_id;
        let buffer_chunk = Arc::clone(&chunk_buffer.proto_chunk);
        let run = async {
            // Generation blocks a chunk worker until the plugin returns, while plugin code may
            // be waiting for that very chunk (`load-chunk`, `get-chunk-at-async`, creating a
            // world). Not taking the root admission lets this call run beside such a call.
            self.plugin
                .store
                .call_guest_unadmitted(move |mut guest| {
                    Box::pin(async move {
                        let (buffer_resource, buffer_rep) = guest.with(|mut store| {
                            let resource = store.data_mut().add(chunk_buffer)?;
                            let rep = resource.rep();
                            Ok::<_, wasmtime::Error>((resource, rep))
                        })?;
                        let result = guest
                            .call(function, (generator_id, phase, buffer_resource))
                            .await;
                        // The guest normally drops the buffer itself, and other plugin tasks
                        // run meanwhile, so the slot may hold another resource by now: only
                        // delete it if it is still this buffer.
                        guest.with(|mut store| {
                            let table = &mut store.data_mut().resource_table;
                            let still_ours = table
                                .get::<ChunkBuffer>(&wasmtime::component::Resource::new_borrow(
                                    buffer_rep,
                                ))
                                .is_ok_and(|buffer| {
                                    Arc::ptr_eq(&buffer.proto_chunk, &buffer_chunk)
                                });
                            if still_ours {
                                let _ = table.delete::<ChunkBuffer>(
                                    wasmtime::component::Resource::new_own(buffer_rep),
                                );
                            }
                        });
                        result
                    })
                })
                .await
        };

        // Chunk generation runs on rayon threads, outside the Tokio runtime.
        let result = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| self.server.runtime.block_on(run))
        } else {
            self.server.runtime.block_on(run)
        };
        if let Err(error) = result {
            let failures = self
                .failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            // The first failures in full, then a reminder now and then: a dead plugin store
            // fails every chunk.
            if failures <= 3 || failures.is_power_of_two() {
                tracing::error!(
                    "Plugin chunk generator {generator_id} failed in phase {phase:?} ({failures} failure(s) so far); the phase is skipped: {error:#}"
                );
            }
        }
    }
}

/// A plugin populator: the generator's `surface` or `features` phase on vanilla-generated
/// chunks.
pub struct WasmChunkPopulator(WasmChunkGenerator);

impl pumpkin_world::generation::populator::ChunkPopulator for WasmChunkPopulator {
    fn populate(
        &self,
        stage: pumpkin_world::generation::populator::PopulatorStage,
        cache: &mut GenerationCacheRegion,
    ) {
        use pumpkin_world::generation::populator::PopulatorStage;
        let mid = ((cache.size * cache.size) >> 1) as usize;
        let chunk_stage = cache.chunks[mid].get_proto_chunk().stage;
        match stage {
            PopulatorStage::Surface => self.0.invoke_phase(
                pumpkin::plugin::world::GenerationPhase::Surface,
                cache.chunks[mid].get_proto_chunk_mut(),
            ),
            PopulatorStage::Features => self
                .0
                .invoke_region_phase(pumpkin::plugin::world::GenerationPhase::Features, cache),
        }
        cache.chunks[mid].get_proto_chunk_mut().stage = chunk_stage;
    }
}

/// A plugin generator's `biomes` phase on vanilla noise chunks (`noise-with-plugin-biomes`).
pub struct WasmBiomePopulator(pub WasmChunkGenerator);

impl pumpkin_world::generation::populator::BiomePopulator for WasmBiomePopulator {
    fn populate_biomes(&self, chunk: &mut pumpkin_world::ProtoChunk) {
        let stage = chunk.stage;
        self.0
            .invoke_phase(pumpkin::plugin::world::GenerationPhase::Biomes, chunk);
        chunk.stage = stage;
    }
}

impl pumpkin_world::generation::generator::CustomChunkGenerator for WasmChunkGenerator {
    fn dimension(&self) -> &pumpkin_data::dimension::Dimension {
        &self.dimension
    }

    fn seed(&self) -> u64 {
        self.seed
    }

    fn global_structure_cache(
        &self,
    ) -> Option<&pumpkin_world::generation::structure::placement::GlobalStructureCache> {
        match self.kept() {
            Some((parts, vanilla)) if parts.structures => Some(&vanilla.global_structure_cache),
            _ => None,
        }
    }

    fn set_structure_starts(&self, chunk: &mut pumpkin_world::ProtoChunk) {
        if let Some((parts, vanilla)) = self.kept()
            && parts.structures
        {
            chunk.set_structure_starts(vanilla);
        }
    }

    fn set_structure_references(&self, chunk: &mut pumpkin_world::ProtoChunk) {
        if let Some((parts, vanilla)) = self.kept()
            && parts.structures
        {
            chunk.set_structure_references(vanilla);
        }
    }

    fn step_to_biomes(&self, chunk: &mut pumpkin_world::ProtoChunk) {
        self.invoke_phase(pumpkin::plugin::world::GenerationPhase::Biomes, chunk);
        chunk.stage = pumpkin_world::chunk_system::StagedChunkEnum::Biomes;
    }

    fn step_to_noise(&self, chunk: &mut pumpkin_world::ProtoChunk) {
        self.invoke_phase(pumpkin::plugin::world::GenerationPhase::Noise, chunk);
        chunk.stage = pumpkin_world::chunk_system::StagedChunkEnum::Noise;
    }

    fn step_to_surface(&self, chunk: &mut pumpkin_world::ProtoChunk) {
        self.invoke_phase(pumpkin::plugin::world::GenerationPhase::Surface, chunk);
        chunk.stage = pumpkin_world::chunk_system::StagedChunkEnum::Surface;
    }

    fn step_to_carvers(&self, chunk: &mut pumpkin_world::ProtoChunk) {
        match self.kept() {
            // Vanilla carvers also prime the heightmaps from the plugin's blocks.
            Some((parts, vanilla)) if parts.carvers => chunk.step_to_carvers(vanilla),
            _ => {
                // Heightmaps from the plugin's blocks, as vanilla has them before features.
                chunk.prime_heightmaps();
                chunk.snapshot_wg_heightmaps();
            }
        }
        chunk.stage = pumpkin_world::chunk_system::StagedChunkEnum::Carvers;
    }

    fn step_to_features(
        &self,
        cache: &mut pumpkin_world::chunk_system::generation_cache::Cache,
        block_registry: &dyn pumpkin_world::world::WorldPortalExt,
    ) {
        if let Some((parts, vanilla)) = self.kept()
            && (parts.features || parts.structures)
        {
            pumpkin_world::ProtoChunk::generate_features_and_structure_filtered(
                cache,
                block_registry,
                &vanilla.random_config,
                parts.features,
                parts.structures,
            );
        }
        self.invoke_region_phase(pumpkin::plugin::world::GenerationPhase::Features, cache);
        let mid = ((cache.size * cache.size) >> 1) as usize;
        cache.chunks[mid].get_proto_chunk_mut().stage =
            pumpkin_world::chunk_system::StagedChunkEnum::Features;
    }
}

#[must_use]
pub const fn from_wit_sound_category(
    category: pumpkin::plugin::sounds::SoundCategory,
) -> pumpkin_data::sound::SoundCategory {
    match category {
        pumpkin::plugin::sounds::SoundCategory::Master => {
            pumpkin_data::sound::SoundCategory::Master
        }
        pumpkin::plugin::sounds::SoundCategory::Music => pumpkin_data::sound::SoundCategory::Music,
        pumpkin::plugin::sounds::SoundCategory::Records => {
            pumpkin_data::sound::SoundCategory::Records
        }
        pumpkin::plugin::sounds::SoundCategory::Weather => {
            pumpkin_data::sound::SoundCategory::Weather
        }
        pumpkin::plugin::sounds::SoundCategory::Blocks => {
            pumpkin_data::sound::SoundCategory::Blocks
        }
        pumpkin::plugin::sounds::SoundCategory::Hostile => {
            pumpkin_data::sound::SoundCategory::Hostile
        }
        pumpkin::plugin::sounds::SoundCategory::Neutral => {
            pumpkin_data::sound::SoundCategory::Neutral
        }
        pumpkin::plugin::sounds::SoundCategory::Players => {
            pumpkin_data::sound::SoundCategory::Players
        }
        pumpkin::plugin::sounds::SoundCategory::Ambient => {
            pumpkin_data::sound::SoundCategory::Ambient
        }
        pumpkin::plugin::sounds::SoundCategory::Voice => pumpkin_data::sound::SoundCategory::Voice,
        pumpkin::plugin::sounds::SoundCategory::Ui => pumpkin_data::sound::SoundCategory::Ui,
    }
}

pub(crate) fn to_wit_biome(
    biome: &pumpkin_data::biome::Biome,
) -> wasmtime::Result<pumpkin::plugin::biomes::Biome> {
    let name = biome
        .registry_id
        .strip_prefix("minecraft:")
        .unwrap_or(biome.registry_id);
    let index = pumpkin_data::biome::Biome::ALL
        .binary_search_by_key(&name, |b| b.registry_id)
        .map_err(|_| wasmtime::Error::msg(format!("Unknown biome: {}", biome.registry_id)))?;

    // SAFETY: The WIT enum is generated with variants matching Biome::ALL in alphabetical order.
    Ok(unsafe { std::mem::transmute::<u8, pumpkin::plugin::biomes::Biome>(index as u8) })
}

pub(crate) fn from_wit_biome(
    biome: pumpkin::plugin::biomes::Biome,
) -> wasmtime::Result<&'static pumpkin_data::biome::Biome> {
    let index = biome as usize;
    pumpkin_data::biome::Biome::ALL
        .get(index)
        .copied()
        .ok_or_else(|| wasmtime::Error::msg(format!("Invalid biome index: {index}")))
}

const fn wit_block_pos(pos: &crate::pumpkin::plugin::common::BlockPos) -> BlockPos {
    BlockPos(pumpkin_util::math::vector3::Vector3::new(
        pos.x, pos.y, pos.z,
    ))
}

/// The chunk biome id of a vanilla or registered custom biome. Network biome palettes hold at
/// most 7 bits, so custom biomes past id 127 cannot be placed.
fn biome_id_by_name(name: &str) -> Result<u8, String> {
    if let Some(biome) =
        pumpkin_data::biome::Biome::from_name(name.strip_prefix("minecraft:").unwrap_or(name))
    {
        return Ok(biome.id);
    }
    let id = pumpkin_data::dynamic_registry::raw_id("worldgen/biome", name)
        .ok_or_else(|| format!("unknown biome '{name}'"))?;
    u8::try_from(id)
        .ok()
        .filter(|id| *id < 128)
        .ok_or_else(|| format!("biome '{name}' has id {id}; at most 127 biomes can be placed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wit_particle_ids_match_internal_particle_ids() {
        let cases = [
            (
                pumpkin::plugin::particles::Particle::AngryVillager,
                pumpkin_data::particle::Particle::AngryVillager,
            ),
            (
                pumpkin::plugin::particles::Particle::HappyVillager,
                pumpkin_data::particle::Particle::HappyVillager,
            ),
            (
                pumpkin::plugin::particles::Particle::SulfurCubeGoo,
                pumpkin_data::particle::Particle::SulfurCubeGoo,
            ),
        ];

        for (wit, internal) in cases {
            assert_eq!(
                pumpkin_data::particle::Particle::from_id(wit as u16),
                Some(internal)
            );
        }
    }
}
