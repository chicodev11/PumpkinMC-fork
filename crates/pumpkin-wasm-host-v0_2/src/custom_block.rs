//! WIT `custom-block`: custom block registration and state queries.

use pumpkin_data::block_properties::{EnumVariants, NoteblockInstrument};
use pumpkin_data::block_state::PistonBehavior;
use pumpkin_data::custom_block::{
    self, BlockDefinition, CustomBlockInfo, CustomShape, CustomSoundGroup, StateDefinition,
};
use pumpkin_data::{Block, BlockStateId};
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_wasm_host_common::state::PluginHostState;

use crate::pumpkin::plugin::custom_block::{
    BlockDefinition as WitBlockDefinition, BlockShape, BlockStateDefinition, Host, PistonReaction,
    RegisteredBlock, SoundGroup,
};

fn to_shape(shape: BlockShape) -> CustomShape {
    match shape {
        BlockShape::Empty => CustomShape::Empty,
        BlockShape::FullCube => CustomShape::FullCube,
        BlockShape::Boxes(boxes) => CustomShape::Boxes(
            boxes
                .into_iter()
                .map(|b| {
                    BoundingBox::new_array([b.min_x, b.min_y, b.min_z], [b.max_x, b.max_y, b.max_z])
                })
                .collect(),
        ),
    }
}

const fn to_piston(reaction: PistonReaction) -> PistonBehavior {
    match reaction {
        PistonReaction::Normal => PistonBehavior::Normal,
        PistonReaction::Destroy => PistonBehavior::Destroy,
        PistonReaction::Block => PistonBehavior::Block,
        PistonReaction::Ignore => PistonBehavior::Ignore,
        PistonReaction::PushOnly => PistonBehavior::PushOnly,
    }
}

fn to_sounds(group: SoundGroup) -> CustomSoundGroup {
    CustomSoundGroup {
        volume: group.volume,
        pitch: group.pitch,
        break_sound: group.break_sound,
        step_sound: group.step_sound,
        place_sound: group.place_sound,
        hit_sound: group.hit_sound,
        fall_sound: group.fall_sound,
    }
}

fn to_instrument(name: &str) -> Result<NoteblockInstrument, String> {
    (0..NoteblockInstrument::variant_count())
        .map(NoteblockInstrument::from_index)
        .find(|instrument| instrument.to_value() == name)
        .ok_or_else(|| format!("unknown note block instrument `{name}`"))
}

fn to_state(def: BlockStateDefinition) -> Result<StateDefinition, String> {
    let client_state = BlockStateId::new(def.client_state)
        .ok_or_else(|| format!("client state {} does not exist", def.client_state))?;
    Ok(StateDefinition {
        client_state,
        hardness: def.hardness,
        blast_resistance: def.resistance,
        luminance: def.luminance,
        opacity: def.opacity,
        collision: to_shape(def.collision),
        outline: def.outline.map(to_shape),
        piston_behavior: to_piston(def.piston),
        sound_group: def.sound_group,
        custom_sounds: def.custom_sounds.map(to_sounds),
        instrument: def
            .instrument
            .as_deref()
            .map_or(Ok(NoteblockInstrument::Harp), to_instrument)?,
        map_color: def.map_color,
        loot_table: def.loot_table,
        tool_required: def.requires_tool,
        replaceable: def.replaceable,
        burnable: def.burnable,
        random_ticks: def.random_ticks,
        redstone_conductor: def.redstone_conductor,
        occludes: def.occludes,
    })
}

/// Converts a WIT definition; `owner` is the registering plugin.
pub fn to_definition(
    def: WitBlockDefinition,
    owner: Option<String>,
) -> Result<BlockDefinition, String> {
    let item_id = match def.item.as_deref() {
        None => 0,
        Some(name) => {
            pumpkin_data::item::Item::from_registry_key(
                name.strip_prefix("minecraft:").unwrap_or(name),
            )
            .ok_or_else(|| format!("unknown item `{name}`"))?
            .id
        }
    };
    Ok(BlockDefinition {
        name: def.id,
        properties: def
            .properties
            .into_iter()
            .map(|p| (p.name, p.values))
            .collect(),
        default_state: def.default_state,
        states: def
            .states
            .into_iter()
            .map(to_state)
            .collect::<Result<_, _>>()?,
        slipperiness: def.friction,
        velocity_multiplier: def.speed_factor,
        jump_velocity_multiplier: def.jump_factor,
        item_id,
        flammable: def.flammable,
        tags: def.tags,
        owner,
        block_entity: def.block_entity,
    })
}

pub fn to_registered(info: &CustomBlockInfo) -> RegisteredBlock {
    let block = info.block;
    RegisteredBlock {
        id: block.name.to_string(),
        block_id: block.id.as_u16(),
        first_state: block.states[0].id.as_u16(),
        state_count: block.states.len() as u16,
        default_state: block.default_state.id.as_u16(),
    }
}

impl Host for PluginHostState {
    fn register_block(
        &mut self,
        definition: WitBlockDefinition,
    ) -> wasmtime::Result<Result<RegisteredBlock, String>> {
        let callbacks = to_callback_bits(definition.callbacks);
        let def = match to_definition(definition, self.name.clone()) {
            Ok(def) => def,
            Err(error) => return Ok(Err(error)),
        };
        let info = match custom_block::register(&def) {
            Ok(info) => info,
            Err(error) => return Ok(Err(error.to_string())),
        };
        if callbacks != 0
            && let (Some(plugin), Some(server)) = (self.plugin.clone(), self.server.as_ref())
        {
            pumpkin_core::block::custom::set_handler(
                info.block,
                callbacks,
                Arc::new(WasmBlockHandler {
                    plugin,
                    server: Arc::downgrade(server),
                }),
            );
        }
        Ok(Ok(to_registered(info)))
    }

    fn register_block_entity_type(
        &mut self,
        definition: crate::pumpkin::plugin::custom_block::BlockEntityTypeDefinition,
    ) -> wasmtime::Result<Result<(), String>> {
        use pumpkin_core::block::entities::custom::{self as be, CustomBlockEntityType};
        let flags = definition.callbacks;
        let pairs = [
            (BlockEntityCallbacks::ON_TICK, be::callback::TICK),
            (BlockEntityCallbacks::ON_LOAD, be::callback::LOAD),
            (BlockEntityCallbacks::ON_UNLOAD, be::callback::UNLOAD),
            (BlockEntityCallbacks::ON_REMOVE, be::callback::REMOVE),
        ];
        let callbacks = pairs
            .into_iter()
            .filter(|(flag, _)| flags.contains(*flag))
            .fold(0, |bits, (_, bit)| bits | bit);
        let handler: Option<Arc<dyn be::CustomBlockEntityHandler>> =
            match (callbacks != 0, self.plugin.clone(), self.server.as_ref()) {
                (true, Some(plugin), Some(server)) => Some(Arc::new(WasmBlockEntityHandler {
                    plugin,
                    server: Arc::downgrade(server),
                })),
                _ => None,
            };
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        Ok(be::register_type(CustomBlockEntityType {
            id: leak(definition.id),
            client_type: definition.client_type.map(leak),
            callbacks,
            handler,
            owner: self.name.clone(),
        })
        .map(|_| ())
        .map_err(|error| error.to_string()))
    }

    fn get_block_entity_type(
        &mut self,
        world: Resource<WitWorld>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<Option<String>> {
        let world = self.get(&world)?;
        Ok(custom_block_entity(world, pos).map(|entity| entity.resource_location().to_string()))
    }

    fn get_block_entity_data(
        &mut self,
        world: Resource<WitWorld>,
        pos: WitBlockPos,
    ) -> wasmtime::Result<Option<crate::pumpkin::plugin::common::NbtTree>> {
        let world = self.get(&world)?;
        Ok(custom_block_entity(world, pos).map(|entity| {
            crate::common::to_wit_nbt_tree(pumpkin_nbt::tag::NbtTag::Compound(entity.data()))
        }))
    }

    fn set_block_entity_data(
        &mut self,
        world: Resource<WitWorld>,
        pos: WitBlockPos,
        data: crate::pumpkin::plugin::common::NbtTree,
    ) -> wasmtime::Result<Result<bool, String>> {
        let world = self.get(&world)?;
        let compound = match crate::common::from_wit_nbt_tree(&data) {
            Ok(pumpkin_nbt::tag::NbtTag::Compound(compound)) => compound,
            Ok(_) => return Ok(Err("block entity data must be a compound".to_string())),
            Err(error) => return Ok(Err(error)),
        };
        let position = pumpkin_util::math::position::BlockPos::new(pos.x, pos.y, pos.z);
        let Some(entity) = world.get_block_entity(&position) else {
            return Ok(Ok(false));
        };
        let Some(custom) = entity
            .as_any()
            .downcast_ref::<pumpkin_core::block::entities::custom::CustomBlockEntity>()
        else {
            return Ok(Ok(false));
        };
        custom.set_data(compound);
        if custom.chunk_data_nbt().is_some() {
            world.update_block_entity(&entity);
        }
        Ok(Ok(true))
    }

    fn register_item(
        &mut self,
        id: String,
        item: Resource<WitItemStack>,
    ) -> wasmtime::Result<Result<(), String>> {
        let stack = self.get(&item)?;
        let Ok(stack) = stack.try_lock() else {
            return Ok(Err("item stack is in use".to_string()));
        };
        Ok(pumpkin_data::custom_item::register(&id, stack.clone()))
    }

    fn set_client_item_view(
        &mut self,
        key: String,
        value: String,
        view: Resource<WitItemStack>,
    ) -> wasmtime::Result<Result<(), String>> {
        let stack = self.get(&view)?;
        let Ok(stack) = stack.try_lock() else {
            return Ok(Err("item stack is in use".to_string()));
        };
        if stack.is_empty() {
            return Ok(Err("the view stack is empty".to_string()));
        }
        pumpkin_data::custom_item::set_client_view(&key, &value, stack.clone());
        Ok(Ok(()))
    }

    fn remove_client_item_view(&mut self, key: String, value: String) -> wasmtime::Result<bool> {
        Ok(pumpkin_data::custom_item::remove_client_view(&key, &value))
    }

    fn unregister_item(&mut self, id: String) -> wasmtime::Result<bool> {
        Ok(pumpkin_data::custom_item::unregister(&id))
    }

    fn get_item(&mut self, id: String) -> wasmtime::Result<Option<Resource<WitItemStack>>> {
        pumpkin_data::custom_item::get(&id)
            .map(|stack| wit_item(self, stack))
            .transpose()
    }

    fn list_items(&mut self) -> wasmtime::Result<Vec<String>> {
        Ok(pumpkin_data::custom_item::ids())
    }

    fn register_loot_table(
        &mut self,
        id: String,
        json: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let Some(server) = self.server.as_ref() else {
            return Ok(Err("server not available".to_string()));
        };
        Ok(server
            .datapack_manager
            .register_plugin_loot_table(&id, &json))
    }

    fn unregister_loot_table(&mut self, id: String) -> wasmtime::Result<bool> {
        Ok(self
            .server
            .as_ref()
            .is_some_and(|server| server.datapack_manager.unregister_plugin_loot_table(&id)))
    }

    fn is_registration_open(&mut self) -> wasmtime::Result<bool> {
        Ok(!custom_block::is_frozen())
    }

    fn get_block(&mut self, id: String) -> wasmtime::Result<Option<RegisteredBlock>> {
        Ok(custom_block::block_by_name(&id)
            .and_then(|block| custom_block::block_info(block.id))
            .map(to_registered))
    }

    fn list_blocks(&mut self) -> wasmtime::Result<Vec<RegisteredBlock>> {
        Ok(custom_block::blocks()
            .into_iter()
            .map(to_registered)
            .collect())
    }

    fn get_state(
        &mut self,
        id: String,
        properties: Vec<(String, String)>,
    ) -> wasmtime::Result<Option<u16>> {
        Ok(custom_block::resolve_state(
            &id,
            properties.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        )
        .map(BlockStateId::as_u16))
    }

    fn describe_state(
        &mut self,
        state: u16,
    ) -> wasmtime::Result<Option<(String, Vec<(String, String)>)>> {
        Ok(BlockStateId::new(state).map(|state| {
            let (name, properties) = custom_block::state_name_and_properties(state);
            (
                name,
                properties
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            )
        }))
    }

    fn with_property(
        &mut self,
        state: u16,
        name: String,
        value: String,
    ) -> wasmtime::Result<Option<u16>> {
        let Some(state) = BlockStateId::new(state) else {
            return Ok(None);
        };
        if state.is_custom() {
            return Ok(custom_block::with_property(state, &name, &value).map(BlockStateId::as_u16));
        }
        let block = Block::from_state_id(state);
        Ok(block
            .parse_property(&name, &value)
            .ok()
            .map(|property| block.apply_properties(state, &[property]).as_u16()))
    }

    fn is_custom_state(&mut self, state: u16) -> wasmtime::Result<bool> {
        Ok(BlockStateId::new(state).is_some_and(BlockStateId::is_custom))
    }

    fn get_client_state(&mut self, state: u16) -> wasmtime::Result<u16> {
        Ok(BlockStateId::new(state).map_or(state, |state| {
            pumpkin_data::block_remap::client_state(state.as_u16())
        }))
    }
}

// --- behaviour callbacks ---

use std::sync::{Arc, Weak};

use pumpkin_core::block::custom::{
    BlockCall, BlockCallResult, CustomBlockHandler, UseResult, callback,
};
use pumpkin_core::server::Server;
use pumpkin_wasm_host_common::plugin::WasmPlugin;
use wasmtime::component::Resource;

use crate::pumpkin::plugin::common::BlockPos as WitBlockPos;
use crate::pumpkin::plugin::custom_block::{
    BlockCall as WitBlockCall, BlockCallResult as WitBlockCallResult, BlockCallbacks, BlockContext,
    DropsContext, EntityContext, NeighborContext, PlaceContext, PlacedContext, PlayerContext,
    RemovedContext, UseContext, UseResult as WitUseResult,
};
use crate::pumpkin::plugin::item_stack::ItemStack as WitItemStack;

/// Maps the WIT callback flags to the server's callback bits.
#[must_use]
pub fn to_callback_bits(flags: BlockCallbacks) -> u32 {
    let pairs = [
        (BlockCallbacks::ON_USE, callback::USE),
        (BlockCallbacks::CAN_PLACE, callback::CAN_PLACE),
        (BlockCallbacks::PLACEMENT_STATE, callback::PLACEMENT_STATE),
        (BlockCallbacks::ON_PLACED, callback::PLACED),
        (BlockCallbacks::ON_BROKEN, callback::BROKEN),
        (BlockCallbacks::DROPS, callback::DROPS),
        (
            BlockCallbacks::ON_NEIGHBOR_UPDATE,
            callback::NEIGHBOR_UPDATE,
        ),
        (BlockCallbacks::ON_RANDOM_TICK, callback::RANDOM_TICK),
        (BlockCallbacks::ON_SCHEDULED_TICK, callback::SCHEDULED_TICK),
        (BlockCallbacks::ON_ENTITY_INSIDE, callback::ENTITY_INSIDE),
        (BlockCallbacks::ON_STEPPED_ON, callback::STEPPED_ON),
        (BlockCallbacks::ON_EXPLODED, callback::EXPLODED),
        (BlockCallbacks::ON_ATTACKED, callback::ATTACKED),
        (BlockCallbacks::ON_REMOVED, callback::REMOVED),
        (BlockCallbacks::CAN_BONEMEAL, callback::CAN_BONEMEAL),
        (BlockCallbacks::ON_BONEMEAL, callback::BONEMEAL),
    ];
    pairs
        .into_iter()
        .filter(|(flag, _)| flags.contains(*flag))
        .fold(0, |bits, (_, bit)| bits | bit)
}

/// Sends a plugin's custom block callbacks to its `handle-block-callback` export.
pub struct WasmBlockHandler {
    pub plugin: Weak<WasmPlugin>,
    pub server: Weak<Server>,
}

const fn wit_pos(pos: pumpkin_util::math::position::BlockPos) -> WitBlockPos {
    WitBlockPos {
        x: pos.0.x,
        y: pos.0.y,
        z: pos.0.z,
    }
}

fn wit_item(
    state: &mut PluginHostState,
    item: pumpkin_data::item_stack::ItemStack,
) -> wasmtime::Result<Resource<WitItemStack>> {
    state.add::<WitItemStack>(Arc::new(tokio::sync::Mutex::new(item)))
}

#[expect(clippy::too_many_lines, reason = "one arm per callback")]
fn to_wit_call(state: &mut PluginHostState, call: BlockCall) -> wasmtime::Result<WitBlockCall> {
    use crate::mob::to_wit_block_direction as dir;
    Ok(match call {
        BlockCall::Use {
            world,
            position,
            state: block_state,
            player,
            item,
            face,
            cursor,
        } => WitBlockCall::OnUse(UseContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            player: state.add(player)?,
            item: wit_item(state, item)?,
            face: dir(face),
            cursor_x: cursor.x,
            cursor_y: cursor.y,
            cursor_z: cursor.z,
        }),
        BlockCall::CanPlace {
            world,
            position,
            state: block_state,
            player,
        } => WitBlockCall::CanPlace(PlaceContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            player: player.map(|p| state.add(p)).transpose()?,
            face: None,
        }),
        BlockCall::PlacementState {
            world,
            position,
            state: block_state,
            player,
            face,
            cursor: _,
        } => WitBlockCall::PlacementState(PlaceContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            player: Some(state.add(player)?),
            face: Some(dir(face)),
        }),
        BlockCall::Placed {
            world,
            position,
            state: block_state,
            old_state,
        } => WitBlockCall::OnPlaced(PlacedContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            old_state: old_state.as_u16(),
        }),
        BlockCall::Broken {
            world,
            position,
            state: block_state,
            player,
        } => WitBlockCall::OnBroken(PlayerContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            player: state.add(player)?,
        }),
        BlockCall::Attacked {
            world,
            position,
            state: block_state,
            player,
        } => WitBlockCall::OnAttacked(PlayerContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            player: state.add(player)?,
        }),
        BlockCall::Drops {
            world,
            position,
            state: block_state,
            tool,
            drops,
        } => WitBlockCall::Drops(DropsContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            tool: wit_item(state, tool)?,
            drops: drops
                .into_iter()
                .map(|stack| wit_item(state, stack))
                .collect::<wasmtime::Result<_>>()?,
        }),
        BlockCall::NeighborUpdate {
            world,
            position,
            state: block_state,
            source_block,
        } => WitBlockCall::OnNeighborUpdate(NeighborContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            source_block: source_block.registry_name(),
        }),
        BlockCall::RandomTick {
            world,
            position,
            state: block_state,
        } => WitBlockCall::OnRandomTick(BlockContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
        }),
        BlockCall::ScheduledTick {
            world,
            position,
            state: block_state,
        } => WitBlockCall::OnScheduledTick(BlockContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
        }),
        BlockCall::Exploded {
            world,
            position,
            state: block_state,
        } => WitBlockCall::OnExploded(BlockContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
        }),
        BlockCall::EntityInside {
            world,
            position,
            state: block_state,
            entity,
        } => WitBlockCall::OnEntityInside(EntityContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            entity: state.add(entity)?,
        }),
        BlockCall::SteppedOn {
            world,
            position,
            state: block_state,
            entity,
        } => WitBlockCall::OnSteppedOn(EntityContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            entity: state.add(entity)?,
        }),
        BlockCall::Removed {
            world,
            position,
            state: block_state,
            moved,
        } => WitBlockCall::OnRemoved(RemovedContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
            moved,
        }),
        BlockCall::CanBonemeal {
            world,
            position,
            state: block_state,
        } => WitBlockCall::CanBonemeal(BlockContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
        }),
        BlockCall::Bonemeal {
            world,
            position,
            state: block_state,
        } => WitBlockCall::OnBonemeal(BlockContext {
            world: state.add(world)?,
            pos: wit_pos(position),
            state: block_state.as_u16(),
        }),
    })
}

/// A plugin's answer, with item stacks taken out of the resource table.
enum Answer {
    None,
    Use(UseResult),
    Allowed(bool),
    State(u16),
    Drops(Vec<Arc<tokio::sync::Mutex<pumpkin_data::item_stack::ItemStack>>>),
}

impl CustomBlockHandler for WasmBlockHandler {
    fn call(
        &self,
        block: &'static pumpkin_data::custom_block::CustomBlockInfo,
        call: BlockCall,
    ) -> BlockCallResult {
        let Some(plugin) = self.plugin.upgrade() else {
            return BlockCallResult::None;
        };
        let Some(server) = self.server.upgrade() else {
            return BlockCallResult::None;
        };
        let name = block.block.name.to_string();
        let run = async move {
            let function = plugin
                .instance::<crate::Plugin>()
                .func_handle_block_callback();
            let result = plugin
                .store
                .call_guest(move |mut guest| {
                    Box::pin(async move {
                        let (server_resource, wit_call) = guest.with(|mut store| {
                            let data = store.data_mut();
                            let server = data.server.clone().ok_or_else(|| {
                                wasmtime::Error::msg("Wasm plugin server is not available")
                            })?;
                            let server_resource =
                                data.add::<crate::pumpkin::plugin::server::Server>(server)?;
                            let wit_call = to_wit_call(data, call)?;
                            Ok::<_, wasmtime::Error>((server_resource, wit_call))
                        })?;
                        let (answer,) = guest
                            .call(function, (server_resource, name, wit_call))
                            .await?;
                        guest.with(|mut store| {
                            let data = store.data_mut();
                            Ok::<_, wasmtime::Error>(match answer {
                                WitBlockCallResult::None => Answer::None,
                                WitBlockCallResult::UseResult(result) => {
                                    Answer::Use(match result {
                                        WitUseResult::Pass => UseResult::Pass,
                                        WitUseResult::Success => UseResult::Success,
                                        WitUseResult::Consume => UseResult::Consume,
                                        WitUseResult::Fail => UseResult::Fail,
                                    })
                                }
                                WitBlockCallResult::Allowed(allowed) => Answer::Allowed(allowed),
                                WitBlockCallResult::State(state) => Answer::State(state),
                                WitBlockCallResult::Drops(drops) => Answer::Drops(
                                    drops
                                        .into_iter()
                                        .map(|stack| data.take::<WitItemStack>(stack))
                                        .collect::<wasmtime::Result<_>>()?,
                                ),
                            })
                        })
                    })
                })
                .await;
            match result {
                Ok(Answer::None) => BlockCallResult::None,
                Ok(Answer::Use(result)) => BlockCallResult::Use(result),
                Ok(Answer::Allowed(allowed)) => BlockCallResult::Allowed(allowed),
                Ok(Answer::State(state)) => pumpkin_data::BlockStateId::new(state)
                    .map_or(BlockCallResult::None, BlockCallResult::State),
                Ok(Answer::Drops(drops)) => {
                    let mut stacks = Vec::with_capacity(drops.len());
                    for stack in drops {
                        stacks.push(stack.lock().await.clone());
                    }
                    BlockCallResult::Drops(stacks)
                }
                Err(error) => {
                    tracing::error!(%error, "Wasm custom block callback failed");
                    BlockCallResult::None
                }
            }
        };
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| server.runtime.block_on(run))
        } else {
            server.runtime.block_on(run)
        }
    }
}

type WitWorld = crate::pumpkin::plugin::world::World;
use crate::pumpkin::plugin::custom_block::{
    BlockEntityCall as WitBlockEntityCall, BlockEntityCallbacks,
};
use pumpkin_core::block::entities::BlockEntity as _;

fn custom_block_entity(
    world: &pumpkin_core::world::World,
    pos: WitBlockPos,
) -> Option<Arc<pumpkin_core::block::entities::custom::CustomBlockEntity>> {
    let position = pumpkin_util::math::position::BlockPos::new(pos.x, pos.y, pos.z);
    let entity = world.get_block_entity(&position)?;
    let any: Arc<dyn std::any::Any + Send + Sync> = entity;
    any.downcast::<pumpkin_core::block::entities::custom::CustomBlockEntity>()
        .ok()
}

/// Sends a plugin's custom block entity callbacks to its `handle-block-entity-callback` export.
pub struct WasmBlockEntityHandler {
    pub plugin: Weak<WasmPlugin>,
    pub server: Weak<Server>,
}

impl pumpkin_core::block::entities::custom::CustomBlockEntityHandler for WasmBlockEntityHandler {
    fn call(
        &self,
        ty: &'static pumpkin_core::block::entities::custom::CustomBlockEntityType,
        call: pumpkin_core::block::entities::custom::BlockEntityCall,
    ) {
        use pumpkin_core::block::entities::custom::BlockEntityCall;
        let (Some(plugin), Some(server)) = (self.plugin.upgrade(), self.server.upgrade()) else {
            return;
        };
        let type_id = ty.id.to_string();
        let run = async move {
            let function = plugin
                .instance::<crate::Plugin>()
                .func_handle_block_entity_callback();
            let result = plugin
                .store
                .call_guest(move |mut guest| {
                    Box::pin(async move {
                        let (server_resource, wit_call) = guest.with(|mut store| {
                            let data = store.data_mut();
                            let server = data.server.clone().ok_or_else(|| {
                                wasmtime::Error::msg("Wasm plugin server is not available")
                            })?;
                            let server_resource =
                                data.add::<crate::pumpkin::plugin::server::Server>(server)?;
                            let (kind, world, position) = match call {
                                BlockEntityCall::Tick { world, position } => (0, world, position),
                                BlockEntityCall::Load { world, position } => (1, world, position),
                                BlockEntityCall::Unload { world, position } => (2, world, position),
                                BlockEntityCall::Remove { world, position } => (3, world, position),
                            };
                            let context = BlockContext {
                                state: world.get_block_state_id(&position).as_u16(),
                                world: data.add(world)?,
                                pos: wit_pos(position),
                            };
                            let wit_call = match kind {
                                0 => WitBlockEntityCall::OnTick(context),
                                1 => WitBlockEntityCall::OnLoad(context),
                                2 => WitBlockEntityCall::OnUnload(context),
                                _ => WitBlockEntityCall::OnRemove(context),
                            };
                            Ok::<_, wasmtime::Error>((server_resource, wit_call))
                        })?;
                        guest
                            .call(function, (server_resource, type_id, wit_call))
                            .await
                    })
                })
                .await;
            if let Err(error) = result {
                tracing::error!(%error, "Wasm custom block entity callback failed");
            }
        };
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| server.runtime.block_on(run));
        } else {
            server.runtime.block_on(run);
        }
    }
}
