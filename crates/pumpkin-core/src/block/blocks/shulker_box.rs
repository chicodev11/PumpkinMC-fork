use std::sync::Arc;
use std::sync::Mutex;

use crate::block::{
    GetComparatorOutputArgs, GetScreenHandlerFactoryArgs, OnPlaceArgs, OnSyncedBlockEventArgs,
    PlacedArgs,
};
use crate::block::{
    registry::BlockActionResult,
    {BlockBehaviour, NormalUseArgs},
};

use crate::block::entities::shulker_box::ShulkerBoxBlockEntity;
use pumpkin_data::BlockStateId;
use pumpkin_data::translation;
use pumpkin_inventory::Inventory;
use pumpkin_inventory::generic_container_screen_handler::create_generic_9x3;
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_macros::pumpkin_block_from_tag;
use pumpkin_util::text::TextComponent;

struct ShulkerBoxScreenFactory(Arc<dyn Inventory>);

impl ScreenHandlerFactory for ShulkerBoxScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        let handler = create_generic_9x3(sync_id, player_inventory, self.0.clone(), player);
        let screen_handler_arc = Arc::new(Mutex::new(handler));

        Some(screen_handler_arc as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        pumpkin_macros::translate_cross!(
            translation::java::CONTAINER_SHULKERBOX,
            translation::bedrock::CONTAINER_SHULKERBOX
        )
    }
}

#[pumpkin_block_from_tag("minecraft:shulker_boxes")]
pub struct ShulkerBoxBlock;

type EndRodLikeProperties = pumpkin_data::block_properties::EndRodLikeProperties;

impl ShulkerBoxBlock {
    /// Vanilla `ShulkerBoxBlock.canOpen`: a closed box opens only if its lid has room.
    fn can_open(
        world: &Arc<crate::world::World>,
        position: &pumpkin_util::math::position::BlockPos,
    ) -> bool {
        use pumpkin_data::FacingExt;
        let Some(block_entity) = world.get_block_entity(position) else {
            return true;
        };
        let Some(shulker) = block_entity
            .as_any()
            .downcast_ref::<ShulkerBoxBlockEntity>()
        else {
            return true;
        };
        if !shulker.is_closed() {
            return true;
        }
        let state = world.get_block_state(position);
        let facing = EndRodLikeProperties::from_state_id(state.id)
            .facing
            .to_block_direction();
        let lid = crate::block::entities::shulker_box::progress_delta_aabb(
            1.0,
            facing,
            0.0,
            0.5,
            pumpkin_util::math::vector3::Vector3::new(
                f64::from(position.0.x) + 0.5,
                f64::from(position.0.y),
                f64::from(position.0.z) + 0.5,
            ),
        )
        .expand(-1.0e-6, -1.0e-6, -1.0e-6);
        world.is_space_empty(lid)
    }
}

impl BlockBehaviour for ShulkerBoxBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = EndRodLikeProperties::default(args.block);
        props.facing = args.direction.to_facing().opposite();
        props.to_state_id(args.block)
    }

    /// Vanilla `BlockEntity.triggerEvent` through the block event.
    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(shulker) = block_entity
                .as_any()
                .downcast_ref::<ShulkerBoxBlockEntity>()
        {
            return shulker.trigger_event(args.r#type, args.data);
        }
        args.r#type == Self::OPEN_ANIMATION_EVENT_TYPE
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        {
            let barrel_block_entity = ShulkerBoxBlockEntity::new(*args.position);
            args.world.add_block_entity(Arc::new(barrel_block_entity));
        }
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        if !Self::can_open(args.world, args.position) {
            return BlockActionResult::Success;
        }
        if let Some(factory) = self.get_screen_handler_factory(GetScreenHandlerFactoryArgs {
            server: args.server,
            world: args.world,
            block: args.block,
            position: args.position,
            player: args.player,
        }) {
            args.player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::OpenShulkerBox as i32,
                1,
            );
            args.player
                .open_handled_screen(factory.as_ref(), Some(*args.position));
            crate::entity::mob::piglin_ai::anger_nearby_piglins(args.world, args.player, true);
        }

        BlockActionResult::Success
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        let block_entity = args.world.get_block_entity(args.position)?;
        // Vanilla `RandomizableContainer.unpackLootTable` on first open (not for spectators).
        if args.player.gamemode.load() != pumpkin_util::GameMode::Spectator {
            crate::block::blocks::chests::unpack_chest_loot(
                &block_entity,
                args.world,
                args.position,
                Some(args.player),
            );
        }
        let inventory = block_entity.get_inventory()?;
        Some(Box::new(ShulkerBoxScreenFactory(inventory)))
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        crate::block::container_comparator_output(&args)
    }
}

impl ShulkerBoxBlock {
    pub const OPEN_ANIMATION_EVENT_TYPE: u8 = 1;
}
