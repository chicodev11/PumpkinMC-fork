use std::sync::Arc;

use crate::block::entities::BlockEntity;
use crate::block::entities::jukebox::JukeboxBlockEntity;
use crate::block::registry::BlockActionResult;
use crate::block::{
    BlockBehaviour, EmitsRedstonePowerArgs, GetComparatorOutputArgs, GetRedstonePowerArgs,
    NormalUseArgs, OnStateReplacedArgs, PlacedArgs, PlayerPlacedArgs, UseWithItemArgs,
};
use crate::entity::Entity;
use crate::entity::item::ItemEntity;
use crate::world::World;
use pumpkin_data::data_component_impl::JukeboxPlayableImpl;
use pumpkin_data::entity::EntityType;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::jukebox_song::JukeboxSong;
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{Block, BlockStateId, block_properties::JukeboxLikeProperties};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;
use rand::{RngExt, rng};

use tracing::error;

#[pumpkin_block("minecraft:jukebox")]
pub struct JukeboxBlock;

impl JukeboxBlock {
    fn has_record_state(_block: &Block, state_id: BlockStateId) -> bool {
        JukeboxLikeProperties::from_state_id(state_id).has_record
    }

    fn set_record_state(has_record: bool, block: &Block, position: &BlockPos, world: &Arc<World>) {
        let new_state = JukeboxLikeProperties { has_record };
        world.set_block_state(
            position,
            new_state.to_state_id(block),
            BlockFlags::NOTIFY_ALL,
        );
    }

    /// Drops the record from the jukebox - matches vanilla's `JukeboxBlockEntity.dropRecord()`
    /// Spawns item at (pos + 0.5, pos + 1.01, pos + 0.5) with horizontal random offset
    fn drop_record(position: &BlockPos, world: &Arc<World>) {
        if let Some(block_entity) = world.get_block_entity(position)
            && let Some(jukebox_entity) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>()
        {
            let record = jukebox_entity.clear_record();
            if !record.is_empty() {
                // Vanilla: Vec3d.add(pos, 0.5, 1.01, 0.5).addHorizontalRandom(random, 0.7F)
                // addHorizontalRandom adds random in range [-0.35, 0.35] to x and z
                let spawn_pos = Vector3::new(
                    f64::from(position.0.x) + 0.5 + rng().random_range(-0.35..0.35),
                    f64::from(position.0.y) + 1.01,
                    f64::from(position.0.z) + 0.5 + rng().random_range(-0.35..0.35),
                );

                let entity = Entity::new(world.clone(), spawn_pos, &EntityType::ITEM);
                // Vanilla: setToDefaultPickupDelay() = 10 ticks
                let item_entity = Arc::new(ItemEntity::new(entity, record));
                world.spawn_entity(item_entity);
            }
        }
    }

    /// Stops the music and updates block state
    fn stop_playing(block: &Block, position: &BlockPos, world: &Arc<World>) {
        Self::set_record_state(false, block, position, world);
        world.emit_game_event(GameEvent::BlockChange.name(), position.to_centered_f64());
        world.sync_world_event(WorldEvent::SoundStopJukeboxSong, *position, 0);
        world.update_neighbors(position, None);
    }

    /// Starts playing music
    fn start_playing(position: &BlockPos, world: &Arc<World>, song_id: u32) {
        world.sync_world_event(WorldEvent::SoundPlayJukeboxSong, *position, song_id as i32);
    }
}

impl BlockBehaviour for JukeboxBlock {
    /// Called when the jukebox is placed - creates the block entity
    fn placed(&self, args: PlacedArgs<'_>) {
        let block_entity = JukeboxBlockEntity::new(*args.position);
        block_entity.set_world(Arc::downgrade(args.world));
        args.world.add_block_entity(Arc::new(block_entity));
    }

    fn player_placed(&self, args: PlayerPlacedArgs<'_>) {
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(jukebox_entity) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>()
            && !jukebox_entity.get_record().is_empty()
        {
            Self::set_record_state(true, args.block, args.position, args.world);
        }
    }

    /// Called when player right-clicks with empty hand or non-disc item
    /// Vanilla: `JukeboxBlock.onUse()` - drops record if present
    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        let state_id = args.world.get_block_state(args.position).id;

        // Vanilla: if (state.get(HAS_RECORD) && world.getBlockEntity(pos) instanceof JukeboxBlockEntity lv)
        if Self::has_record_state(args.block, state_id) {
            // Drop the record
            Self::drop_record(args.position, args.world);
            // Stop the music and update block state
            Self::stop_playing(args.block, args.position, args.world);
            return BlockActionResult::Success;
        }

        BlockActionResult::Pass
    }

    /// Called when player right-clicks with an item
    /// Vanilla: `JukeboxBlock.onUseWithItem()` -> `JukeboxPlayableComponent.tryPlayStack()`
    fn use_with_item(&self, args: UseWithItemArgs<'_>) -> BlockActionResult {
        let world = args.world;
        let state_id = world.get_block_state(args.position).id;

        // Vanilla: if (state.get(HAS_RECORD)) return PASS_TO_DEFAULT_BLOCK_ACTION
        if Self::has_record_state(args.block, state_id) {
            return BlockActionResult::PassToDefaultBlockAction;
        }

        let item_stack = &mut *args.item_stack;

        // Vanilla: JukeboxPlayableComponent lv = stack.get(DataComponentTypes.JUKEBOX_PLAYABLE)
        let jukebox_playable = item_stack
            .get_data_component::<JukeboxPlayableImpl>()
            .map(|i| i.song);

        // Vanilla: if (lv == null) return PASS_TO_DEFAULT_BLOCK_ACTION
        let Some(jukebox_playable) = jukebox_playable else {
            return BlockActionResult::PassToDefaultBlockAction;
        };

        let Some(jukebox_song) = SongInfo::resolve(jukebox_playable) else {
            error!("Jukebox playable song not registered: {jukebox_playable}");
            return BlockActionResult::PassToDefaultBlockAction;
        };

        // Vanilla: ItemStack lv3 = stack.splitUnlessCreative(1, player)
        let record = item_stack.split_unless_creative(args.player.gamemode.load(), 1);

        // Vanilla: lv4.setStack(lv3)
        if let Some(block_entity) = world.get_block_entity(args.position)
            && let Some(jukebox_entity) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>()
        {
            jukebox_entity.set_world(Arc::downgrade(world));
            jukebox_entity.set_record(record);
            jukebox_entity.start_playing(
                jukebox_song.length_in_ticks + JukeboxBlockEntity::SONG_END_PADDING_TICKS,
            );
        }

        // Update block state to has_record = true
        Self::set_record_state(true, args.block, args.position, world);
        world.emit_game_event(
            GameEvent::BlockChange.name(),
            args.position.to_centered_f64(),
        );

        // Start playing the music (client-side audio) and notify redstone neighbors
        Self::start_playing(args.position, world, jukebox_song.id);
        world.update_neighbors(args.position, None);

        args.player.increment_stat(
            pumpkin_data::statistic::StatisticCategory::Custom,
            pumpkin_data::statistic::CustomStatistic::PlayRecord as i32,
            1,
        );

        // Vanilla `JukeboxPlayable.tryInsertIntoJukebox`
        world.emit_game_event_with_context(
            pumpkin_data::game_event::GameEvent::BlockChange.name(),
            args.position.to_centered_f64(),
            &crate::world::game_event::GameEventContext {
                source_entity: Some(args.player.as_ref()),
            },
        );

        BlockActionResult::Success
    }

    /// Vanilla: `JukeboxBlock.affectNeighborsAfterRemoval()` -> `Containers.updateNeighboursAfterDestroy()`
    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        args.world
            .update_neighbour_for_output_signal(args.position, args.block);
    }

    /// Vanilla: `JukeboxBlock.emitsRedstonePower()` returns true
    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    /// Vanilla: Returns 15 if playing, 0 otherwise
    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        // Vanilla: return world.getBlockEntity(pos) instanceof JukeboxBlockEntity lv && lv.getManager().isPlaying() ? 15 : 0
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(jukebox_entity) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>()
            && jukebox_entity.is_playing()
        {
            15
        } else {
            0
        }
    }

    /// Vanilla: Returns the song's comparator output (0-15)
    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        // Vanilla: return world.getBlockEntity(pos) instanceof JukeboxBlockEntity lv ? lv.getComparatorOutput() : 0
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(jukebox_entity) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>()
        {
            let record = jukebox_entity.get_record();
            // Get the song from the record's jukebox_playable component
            if let Some(playable) = record.get_data_component::<JukeboxPlayableImpl>()
                && let Some(song) = SongInfo::resolve(playable.song)
            {
                return Some(song.comparator_output);
            }
        }
        Some(0)
    }
}

/// What a jukebox needs to know about a song: a vanilla one, or one a datapack or plugin added
/// to the `jukebox_song` registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SongInfo {
    /// Network id (sent with the play world event).
    pub id: u32,
    pub length_in_ticks: u64,
    pub comparator_output: u8,
}

impl SongInfo {
    /// The song a `jukebox_playable` component names.
    #[must_use]
    pub fn resolve(song: &str) -> Option<Self> {
        let vanilla = song
            .strip_prefix("minecraft:")
            .or_else(|| (!song.contains(':')).then_some(song))
            .and_then(JukeboxSong::from_name);
        if let Some(song) = vanilla {
            return Some(Self {
                id: song.get_id(),
                length_in_ticks: song.length_in_ticks(),
                comparator_output: song.comparator_output(),
            });
        }
        let id = pumpkin_data::dynamic_registry::raw_id("jukebox_song", song)?;
        let data = pumpkin_data::dynamic_registry::entry_data("jukebox_song", song)?;
        let mut cursor = std::io::Cursor::new(&data[..]);
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
        let element = pumpkin_nbt::Nbt::read_unnamed(&mut reader).ok()?.root_tag;
        let seconds = element
            .get_float("length_in_seconds")
            .or_else(|| element.get_double("length_in_seconds").map(|s| s as f32))
            .or_else(|| element.get_int("length_in_seconds").map(|s| s as f32))?;
        let comparator = element.get_int("comparator_output").unwrap_or(0);
        Some(Self {
            id,
            // Vanilla `JukeboxSong.lengthInTicks`: Mth.ceil(lengthInSeconds * 20).
            length_in_ticks: (f64::from(seconds) * 20.0).ceil().max(0.0) as u64,
            comparator_output: comparator.clamp(0, 15) as u8,
        })
    }
}

#[cfg(test)]
mod song_tests {
    use super::SongInfo;

    #[test]
    fn plugin_songs_resolve_from_the_registry() {
        let manager = crate::data::datapack::DatapackManager::new();
        manager
            .register_plugin_registry_entry(
                "jukebox_song",
                "testplugin:theme",
                r#"{"sound_event":"testplugin:music.theme","description":"Theme","length_in_seconds":12.5,"comparator_output":7}"#,
            )
            .unwrap();
        assert!(
            manager
                .register_plugin_registry_entry("jukebox_song", "testplugin:theme", "{}")
                .is_err()
        );
        assert!(
            manager
                .register_plugin_registry_entry("jukebox_song", "minecraft:x", "{}")
                .is_err()
        );
        let song = SongInfo::resolve("testplugin:theme").unwrap();
        assert_eq!(song.length_in_ticks, 250);
        assert_eq!(song.comparator_output, 7);
        let vanilla = SongInfo::resolve("minecraft:pigstep").unwrap();
        assert!(song.id >= 22 && vanilla.id < 22);
    }
}
