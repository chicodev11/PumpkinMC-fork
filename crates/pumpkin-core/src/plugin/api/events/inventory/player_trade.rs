use crate::entity::player::Player;
use pumpkin_macros::{Event, cancellable};
use pumpkin_protocol::java::client::play::MerchantOffer;
use std::sync::Arc;

/// A player takes the result of a trade (Paper `PlayerPurchaseEvent` / `PlayerTradeEvent`).
/// Cancelling it stops the trade: nothing is paid, given or counted.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerTradeEvent {
    pub player: Arc<Player>,
    /// The villager or wandering trader; `None` for a plugin merchant screen.
    pub merchant_entity_id: Option<i32>,
    /// The offer's index in the screen's list.
    pub offer_index: u32,
    /// The offer as it is before this trade.
    pub offer: MerchantOffer,
}

impl PlayerTradeEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        merchant_entity_id: Option<i32>,
        offer_index: u32,
        offer: MerchantOffer,
    ) -> Self {
        Self {
            player,
            merchant_entity_id,
            offer_index,
            offer,
            cancelled: false,
        }
    }
}
