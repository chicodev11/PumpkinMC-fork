use crate::block::registry::BlockActionResult;
use crate::block::{BlockBehaviour, GetScreenHandlerFactoryArgs, NormalUseArgs};

use pumpkin_data::translation;
use pumpkin_inventory::cartography_table_screen_handler::CartographyTableScreenHandler;
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::text::TextComponent;
use std::sync::Arc;
use std::sync::Mutex;

#[pumpkin_block("minecraft:cartography_table")]
pub struct CartographyTableBlock;

impl BlockBehaviour for CartographyTableBlock {
    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        if let Some(factory) = self.get_screen_handler_factory(GetScreenHandlerFactoryArgs {
            server: args.server,
            world: args.world,
            block: args.block,
            position: args.position,
            player: args.player,
        }) {
            args.player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::InteractWithCartographyTable as i32,
                1,
            );
            args.player
                .open_handled_screen(factory.as_ref(), Some(*args.position));
        }

        BlockActionResult::Success
    }

    fn get_screen_handler_factory(
        &self,
        _args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        Some(Box::new(CartographyTableScreenFactory))
    }
}

struct CartographyTableScreenFactory;

impl ScreenHandlerFactory for CartographyTableScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        let mut handler = CartographyTableScreenHandler::new(sync_id, player_inventory);
        if let Some(player) = player
            .as_any()
            .downcast_ref::<crate::entity::player::Player>()
        {
            let process_server = player.world().server.clone();
            handler = handler.with_post_process(Arc::new(move |stack| {
                if let Some(server) = process_server.upgrade() {
                    crate::entity::player::map_post_process(&server, stack);
                }
            }));
            let server = player.world().server.clone();
            handler = handler.with_map_info(Arc::new(move |map_id| {
                let map = server.upgrade()?.map_manager.get_map(map_id)?;
                let map = map
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                Some((map.scale, map.locked))
            }));
        }
        let handler: SharedScreenHandler = Arc::new(Mutex::new(handler));
        Some(handler)
    }

    fn get_display_name(&self) -> TextComponent {
        TextComponent::translate(translation::java::CONTAINER_CARTOGRAPHY_TABLE, [])
    }
}
