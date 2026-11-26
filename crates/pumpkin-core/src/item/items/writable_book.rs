use std::any::Any;

use crate::command::resolve_component::{ResolutionContext, resolve_written_book};
use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::item::Item;
use pumpkin_data::statistic::StatisticCategory;
use pumpkin_util::Hand;

pub struct WritableBookItem;

impl ItemMetadata for WritableBookItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::WRITABLE_BOOK.id, Item::WRITTEN_BOOK.id])
    }
}

impl ItemBehaviour for WritableBookItem {
    fn normal_use(&self, item: &Item, player: &Player) {
        self.normal_use_with_hand(item, player, 0.0, 0.0, Hand::Right);
    }

    fn normal_use_with_hand(
        &self,
        item: &Item,
        player: &Player,
        _yaw: f32,
        _pitch: f32,
        hand: Hand,
    ) {
        // The client opens a book and quill by itself when the player uses it.
        // Sending the open-book packet for it makes the client open the book
        // again and it lands in the read-only view, where nothing can be
        // written. A signed book is not opened by the client, so the server has
        // to send the packet for that one.
        if item.id == Item::WRITTEN_BOOK.id {
            // Vanilla `ServerPlayer.openItemGui`: resolve the book's selectors and scores
            // first, for the player as the command source.
            if let Some(player_arc) = player.world().get_player_by_uuid(player.gameprofile.id)
                && let Some(server) = player.world().server.upgrade()
            {
                let source = player_arc.get_command_source(&server);
                let context = ResolutionContext::new(&source);
                let mut stack = player.inventory.get_stack_in_hand(hand);
                if resolve_written_book(&context, &mut stack) {
                    player.inventory.set_stack_in_hand(hand, stack);
                }
            }
            player.open_book(hand);
        }
        player.increment_stat(StatisticCategory::Used, i32::from(item.id), 1);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
