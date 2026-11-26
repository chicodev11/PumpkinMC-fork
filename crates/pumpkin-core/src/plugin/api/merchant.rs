//! Plugin merchant screens (Paper `Bukkit.createMerchant` + `openMerchant`): a villager
//! trading screen with the plugin's trades, not tied to any entity.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use pumpkin_inventory::SimpleInventory;
use pumpkin_inventory::merchant::merchant_screen_handler::MerchantScreenHandler;
use pumpkin_protocol::bedrock::client::CUpdateTrade;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::codec::var_long::VarLong;
use pumpkin_protocol::java::client::play::{CMerchantOffers, MerchantOffer};
use pumpkin_util::text::TextComponent;

use crate::entity::EntityBase;
use crate::entity::passive::wandering_trader::WanderingTraderEntity;
use crate::entity::player::Player;
use crate::plugin::api::events::inventory::player_trade::PlayerTradeEvent;

/// `MerchantScreenHandler::before_trade` that fires [`PlayerTradeEvent`] for the trading
/// player; `merchant_entity_id` is the villager or wandering trader (none for plugin screens).
pub fn trade_event_check(
    merchant_entity_id: Option<i32>,
) -> pumpkin_inventory::merchant::merchant_screen_handler::MerchantTradeCheck {
    Box::new(move |inventory_player, offer_index, offer| {
        let Some(player) = inventory_player.as_any().downcast_ref::<Player>() else {
            return true;
        };
        let Some(server) = player.world().server.upgrade() else {
            return true;
        };
        let Some(player) = server.get_player_by_uuid(player.get_entity().entity_uuid) else {
            return true;
        };
        let mut event = PlayerTradeEvent::new(
            player,
            merchant_entity_id,
            u32::try_from(offer_index).unwrap_or(u32::MAX),
            offer.clone(),
        );
        server.plugin_manager.fire_blocking(&server, &mut event);
        !event.cancelled
    })
}

/// Opens a merchant screen titled `title` with `offers` for `player`.
///
/// Trading works as with a villager: the costs are taken, the result is given and each trade's uses go up until it is
/// out of stock; items left in the cost slots go back to the player on close.
pub fn open_merchant(player: &Arc<Player>, title: &TextComponent, offers: Vec<MerchantOffer>) {
    player.increment_screen_handler_sync_id();
    let sync_id = player.screen_handler_sync_id.load(Ordering::Relaxed);
    let mut handler = MerchantScreenHandler::new(
        sync_id,
        player.inventory(),
        Arc::new(SimpleInventory::new(3)),
        offers.clone(),
    );
    handler.before_trade = Some(trade_event_check(None));
    player.open_handled_screen_direct(Arc::new(std::sync::Mutex::new(handler)), title);

    let bedrock = CUpdateTrade {
        container_id: sync_id,
        r#type: 15,
        size: VarInt(0),
        trader_tier: VarInt(0),
        entity_unique_id: VarLong(i64::from(player.entity_id())),
        last_trading_player: VarLong(i64::from(player.entity_id())),
        display_name: title.clone().to_pretty_console(),
        use_new_trade_screen: true,
        using_economy_trade: true,
        data: WanderingTraderEntity::bedrock_trade_data(&offers),
    };
    // Level 1 without an xp bar: a plain merchant, not a villager.
    let java = CMerchantOffers::new(
        VarInt(i32::from(sync_id)),
        offers,
        VarInt(1),
        VarInt(0),
        false,
        false,
    );
    player.client.try_enqueue_packet_editioned(&java, &bedrock);
}
