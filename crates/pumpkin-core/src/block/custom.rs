//! Behaviour of custom (plugin) blocks, see [`pumpkin_data::custom_block`].
//!
//! Every custom block shares one [`CustomBlockBehaviour`]. It calls the block's
//! [`CustomBlockHandler`] (set by the plugin host) only for the callbacks the block registered,
//! so a block without callbacks never leaves the server, and vanilla blocks never get here.
//! Callbacks run synchronously inside the tick, like vanilla block behaviour.

use std::sync::Arc;

use pumpkin_data::custom_block::{self, CustomBlockInfo};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::{Block, BlockDirection, BlockStateId};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::block::registry::BlockActionResult;
use crate::block::{
    AttackArgs, BlockBehaviour, BlockMetadata, BrokenArgs, CanPlaceAtArgs, ExplodeArgs,
    NormalUseArgs, OnEntityCollisionArgs, OnEntityStepArgs, OnNeighborUpdateArgs, OnPlaceArgs,
    OnScheduledTickArgs, OnStateReplacedArgs, PlacedArgs, RandomTickArgs, UseWithItemArgs,
};
use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::world::World;

/// Callback bits a custom block can register (`CustomBlockInfo::callbacks`).
pub mod callback {
    /// Right click (with or without an item).
    pub const USE: u32 = 1 << 0;
    /// Whether the block may be placed (or stay) at a position.
    pub const CAN_PLACE: u32 = 1 << 1;
    /// The state to place (instead of the default state).
    pub const PLACEMENT_STATE: u32 = 1 << 2;
    /// After the block was placed in the world.
    pub const PLACED: u32 = 1 << 3;
    /// After a player broke the block.
    pub const BROKEN: u32 = 1 << 4;
    /// Replaces the block's loot table drops.
    pub const DROPS: u32 = 1 << 5;
    /// A neighbouring block changed.
    pub const NEIGHBOR_UPDATE: u32 = 1 << 6;
    /// Random tick (the state must also be registered with `random-ticks`).
    pub const RANDOM_TICK: u32 = 1 << 7;
    /// A scheduled block tick fired.
    pub const SCHEDULED_TICK: u32 = 1 << 8;
    /// An entity is inside the block's shape.
    pub const ENTITY_INSIDE: u32 = 1 << 9;
    /// An entity walks on the block.
    pub const STEPPED_ON: u32 = 1 << 10;
    /// The block was destroyed by an explosion.
    pub const EXPLODED: u32 = 1 << 11;
    /// A player started hitting the block.
    pub const ATTACKED: u32 = 1 << 12;
    /// The block was removed or replaced (any cause).
    pub const REMOVED: u32 = 1 << 13;
    /// Whether bone meal works on the block (vanilla `isValidBonemealTarget`).
    pub const CAN_BONEMEAL: u32 = 1 << 14;
    /// Bone meal was used on the block (vanilla `performBonemeal`).
    pub const BONEMEAL: u32 = 1 << 15;
}

/// Result of a use callback (vanilla `InteractionResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseResult {
    /// Not handled: the held item is used next (placing a block against this one, for example).
    Pass,
    /// Handled; swings the hand.
    Success,
    /// Handled without a swing.
    Consume,
    /// Handled as a failure; nothing else happens.
    Fail,
}

/// One callback into a custom block's handler. Values are owned so a plugin host can hand
/// them to its guest.
pub enum BlockCall {
    Use {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        player: Arc<Player>,
        /// The held item (empty when the hand is empty).
        item: ItemStack,
        face: BlockDirection,
        cursor: Vector3<f32>,
    },
    CanPlace {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        player: Option<Arc<Player>>,
    },
    PlacementState {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        player: Arc<Player>,
        face: BlockDirection,
        cursor: Vector3<f32>,
    },
    Placed {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        old_state: BlockStateId,
    },
    Broken {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        player: Arc<Player>,
    },
    Drops {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        tool: ItemStack,
        /// The loot table's drops, which the handler may keep or replace.
        drops: Vec<ItemStack>,
    },
    NeighborUpdate {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        source_block: &'static Block,
    },
    RandomTick {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
    },
    ScheduledTick {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
    },
    EntityInside {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        entity: Arc<dyn EntityBase>,
    },
    SteppedOn {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        entity: Arc<dyn EntityBase>,
    },
    Exploded {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
    },
    Attacked {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        player: Arc<Player>,
    },
    Removed {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
        moved: bool,
    },
    CanBonemeal {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
    },
    Bonemeal {
        world: Arc<World>,
        position: BlockPos,
        state: BlockStateId,
    },
}

/// What a handler answered. A mismatching answer (or `None`) keeps the default.
pub enum BlockCallResult {
    None,
    Use(UseResult),
    Allowed(bool),
    State(BlockStateId),
    Drops(Vec<ItemStack>),
}

/// Answers the callbacks of custom blocks (implemented by the plugin host).
pub trait CustomBlockHandler: Send + Sync {
    fn call(&self, block: &'static CustomBlockInfo, call: BlockCall) -> BlockCallResult;
}

type HandlerBox = Arc<dyn CustomBlockHandler>;

/// Sets the handler and callback bits of a custom block. Returns `false` if the block is not
/// custom or already has a handler.
pub fn set_handler(block: &Block, callbacks: u32, handler: Arc<dyn CustomBlockHandler>) -> bool {
    let Some(info) = custom_block::block_info(block.id) else {
        return false;
    };
    if info.handler.set(Box::new(handler)).is_err() {
        return false;
    }
    info.callbacks
        .store(callbacks, std::sync::atomic::Ordering::Release);
    true
}

/// The handler of a custom block, if it registered `bit`.
#[inline]
fn handler_for(block: &Block, bit: u32) -> Option<(&'static CustomBlockInfo, &'static HandlerBox)> {
    let info = custom_block::block_info(block.id)?;
    if info.callbacks.load(std::sync::atomic::Ordering::Acquire) & bit == 0 {
        return None;
    }
    let handler = info.handler.get()?.downcast_ref::<HandlerBox>()?;
    Some((info, handler))
}

/// Calls the handler of `block` if it registered `bit`.
pub fn call(block: &Block, bit: u32, call: impl FnOnce() -> Option<BlockCall>) -> BlockCallResult {
    let Some((info, handler)) = handler_for(block, bit) else {
        return BlockCallResult::None;
    };
    let Some(call) = call() else {
        return BlockCallResult::None;
    };
    handler.call(info, call)
}

/// Whether `block` registered `bit` (cheap; use before building expensive call data).
#[inline]
#[must_use]
pub fn has_callback(block: &Block, bit: u32) -> bool {
    handler_for(block, bit).is_some()
}

fn entity_arc(world: &World, entity: &dyn EntityBase) -> Option<Arc<dyn EntityBase>> {
    world.get_entity_by_id(entity.get_entity().entity_id)
}

fn world_arc(world: &World) -> Option<Arc<World>> {
    world.server.upgrade().and_then(|server| {
        server
            .worlds
            .load()
            .iter()
            .find(|candidate| std::ptr::eq(Arc::as_ptr(candidate), world))
            .cloned()
    })
}

const fn to_action(result: UseResult) -> BlockActionResult {
    match result {
        UseResult::Pass => BlockActionResult::Pass,
        UseResult::Success => BlockActionResult::Success,
        UseResult::Consume => BlockActionResult::Consume,
        UseResult::Fail => BlockActionResult::Fail,
    }
}

/// The behaviour every custom block shares.
pub struct CustomBlockBehaviour;

impl BlockMetadata for CustomBlockBehaviour {
    fn ids() -> Box<[pumpkin_data::BlockId]> {
        Box::new([])
    }
}

impl BlockBehaviour for CustomBlockBehaviour {
    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        match call(args.block, callback::USE, || {
            Some(BlockCall::Use {
                world: args.world.clone(),
                position: *args.position,
                state: args.world.get_block_state_id(args.position),
                player: args.player.clone(),
                item: ItemStack::EMPTY.clone(),
                face: *args.hit.face,
                cursor: *args.hit.cursor_pos,
            })
        }) {
            BlockCallResult::Use(result) => to_action(result),
            _ => BlockActionResult::Pass,
        }
    }

    fn use_with_item(&self, args: UseWithItemArgs<'_>) -> BlockActionResult {
        match call(args.block, callback::USE, || {
            Some(BlockCall::Use {
                world: args.world.clone(),
                position: *args.position,
                state: args.world.get_block_state_id(args.position),
                player: args.player.clone(),
                item: args.item_stack.clone(),
                face: *args.hit.face,
                cursor: *args.hit.cursor_pos,
            })
        }) {
            BlockCallResult::Use(UseResult::Pass) => BlockActionResult::PassToDefaultBlockAction,
            BlockCallResult::Use(result) => to_action(result),
            _ => BlockActionResult::PassToDefaultBlockAction,
        }
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let default = args.block.default_state.id;
        match call(args.block, callback::PLACEMENT_STATE, || {
            Some(BlockCall::PlacementState {
                world: args.player.world(),
                position: *args.position,
                state: default,
                player: args
                    .player
                    .world()
                    .get_player_by_id(args.player.entity_id())?,
                face: args.direction,
                cursor: args.use_item_on.cursor_pos,
            })
        }) {
            // Only states of this block may be placed.
            BlockCallResult::State(state) if Block::from_state_id(state).id == args.block.id => {
                state
            }
            _ => default,
        }
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        match call(args.block, callback::CAN_PLACE, || {
            let world = args
                .world
                .and_then(world_arc)
                .or_else(|| args.player.map(Player::world))?;
            let player = args
                .player
                .and_then(|player| world.get_player_by_id(player.entity_id()));
            Some(BlockCall::CanPlace {
                world,
                position: *args.position,
                state: args.state.id,
                player,
            })
        }) {
            BlockCallResult::Allowed(allowed) => allowed,
            _ => true,
        }
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        call(args.block, callback::PLACED, || {
            Some(BlockCall::Placed {
                world: args.world.clone(),
                position: *args.position,
                state: args.state_id,
                old_state: args.old_state_id,
            })
        });
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        call(args.block, callback::BROKEN, || {
            Some(BlockCall::Broken {
                world: args.world.clone(),
                position: *args.position,
                state: args.state.id,
                player: args.player.clone(),
            })
        });
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        call(args.block, callback::NEIGHBOR_UPDATE, || {
            Some(BlockCall::NeighborUpdate {
                world: args.world.clone(),
                position: *args.position,
                state: args.world.get_block_state_id(args.position),
                source_block: Block::from_id(args.source_block.id),
            })
        });
    }

    fn is_valid_bonemeal_target(&self, args: crate::block::BonemealArgs<'_>) -> bool {
        // A block with an `on-bonemeal` hook takes bone meal unless `can-bonemeal` says no.
        if !has_callback(args.block, callback::BONEMEAL) {
            return false;
        }
        match call(args.block, callback::CAN_BONEMEAL, || {
            Some(BlockCall::CanBonemeal {
                world: args.world.clone(),
                position: *args.position,
                state: args.state_id,
            })
        }) {
            BlockCallResult::Allowed(allowed) => allowed,
            _ => true,
        }
    }

    fn perform_bonemeal(&self, args: crate::block::BonemealArgs<'_>) {
        call(args.block, callback::BONEMEAL, || {
            Some(BlockCall::Bonemeal {
                world: args.world.clone(),
                position: *args.position,
                state: args.state_id,
            })
        });
    }

    fn random_tick(&self, args: RandomTickArgs<'_>) {
        call(args.block, callback::RANDOM_TICK, || {
            Some(BlockCall::RandomTick {
                world: args.world.clone(),
                position: *args.position,
                state: args.world.get_block_state_id(args.position),
            })
        });
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        call(args.block, callback::SCHEDULED_TICK, || {
            Some(BlockCall::ScheduledTick {
                world: args.world.clone(),
                position: *args.position,
                state: args.world.get_block_state_id(args.position),
            })
        });
    }

    fn on_entity_collision(&self, args: OnEntityCollisionArgs<'_>) {
        call(args.block, callback::ENTITY_INSIDE, || {
            Some(BlockCall::EntityInside {
                world: args.world.clone(),
                position: *args.position,
                state: args.state.id,
                entity: entity_arc(args.world, args.entity)?,
            })
        });
    }

    fn on_entity_step(&self, args: OnEntityStepArgs<'_>) {
        call(args.block, callback::STEPPED_ON, || {
            Some(BlockCall::SteppedOn {
                world: args.world.clone(),
                position: *args.position,
                state: args.state.id,
                entity: entity_arc(args.world, args.entity)?,
            })
        });
    }

    fn explode(&self, args: ExplodeArgs<'_>) {
        call(args.block, callback::EXPLODED, || {
            Some(BlockCall::Exploded {
                world: args.world.clone(),
                position: *args.position,
                state: args.world.get_block_state_id(args.position),
            })
        });
    }

    fn attack(&self, args: AttackArgs<'_>) {
        call(args.block, callback::ATTACKED, || {
            Some(BlockCall::Attacked {
                world: args.world.clone(),
                position: *args.position,
                state: args.state.id,
                player: args.player.clone(),
            })
        });
    }

    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        call(args.block, callback::REMOVED, || {
            Some(BlockCall::Removed {
                world: args.world.clone(),
                position: *args.position,
                state: args.old_state_id,
                moved: args.moved,
            })
        });
    }
}

/// Lets the DROPS callback of a custom block keep or replace `drops`.
pub fn custom_drops(
    world: &Arc<World>,
    block: &Block,
    state: BlockStateId,
    position: &BlockPos,
    tool: Option<&ItemStack>,
    drops: Vec<ItemStack>,
) -> Vec<ItemStack> {
    if !has_callback(block, callback::DROPS) {
        return drops;
    }
    let fallback = drops.clone();
    match call(block, callback::DROPS, || {
        Some(BlockCall::Drops {
            world: world.clone(),
            position: *position,
            state,
            tool: tool.cloned().unwrap_or_else(|| ItemStack::EMPTY.clone()),
            drops,
        })
    }) {
        BlockCallResult::Drops(drops) => drops,
        _ => fallback,
    }
}

/// Which sound of a custom state's string sound group to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKind {
    Break,
    Step,
    Place,
    Hit,
    Fall,
}

/// Plays the resource pack (string id) sound of a custom state.
///
/// Volume and pitch are the group's, scaled by the caller like vanilla scales them for that kind
/// of sound. Returns `false` (and plays nothing) for states without a custom sound group, so
/// callers fall back to the vanilla sound.
pub fn play_custom_sound(
    world: &World,
    state: BlockStateId,
    at: &Vector3<f64>,
    kind: SoundKind,
    volume_scale: f32,
    pitch_scale: f32,
) -> bool {
    if !state.is_custom() {
        return false;
    }
    let Some(sounds) = custom_block::state_info(state).and_then(|info| info.sounds.as_ref()) else {
        return false;
    };
    let name = match kind {
        SoundKind::Break => &sounds.break_sound,
        SoundKind::Step => &sounds.step_sound,
        SoundKind::Place => &sounds.place_sound,
        SoundKind::Hit => &sounds.hit_sound,
        SoundKind::Fall => &sounds.fall_sound,
    };
    if name.is_empty() {
        return true;
    }
    world.play_custom_sound(
        name,
        pumpkin_data::sound::SoundCategory::Blocks,
        at,
        sounds.volume * volume_scale,
        sounds.pitch * pitch_scale,
    );
    true
}

/// [`play_custom_sound`] with vanilla's scaling for `kind` (`SoundType` volume/pitch factors).
pub fn play_custom_block_sound(
    world: &World,
    state: BlockStateId,
    pos: &BlockPos,
    kind: SoundKind,
) -> bool {
    let (volume, pitch) = match kind {
        // Vanilla `LevelRenderer` break and `BlockItem` place: (volume + 1) / 2, pitch * 0.8.
        SoundKind::Break | SoundKind::Place => (0.5, 0.8),
        // Vanilla `MultiPlayerGameMode` hit: (volume + 1) / 8, pitch * 0.5.
        SoundKind::Hit => (0.125, 0.5),
        SoundKind::Step => (0.15, 1.0),
        SoundKind::Fall => (0.5, 0.75),
    };
    play_custom_sound(world, state, &pos.to_centered_f64(), kind, volume, pitch)
}
