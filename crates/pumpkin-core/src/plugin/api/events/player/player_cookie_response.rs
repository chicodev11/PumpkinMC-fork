use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::player::Player;

use super::PlayerEvent;

/// A Java client answered a cookie request (Paper `retrieveCookie`); `payload` is `None` when
/// the client has no cookie under that key.
#[derive(Event, Clone)]
pub struct PlayerCookieResponseEvent {
    pub player: Arc<Player>,
    pub key: String,
    pub payload: Option<Vec<u8>>,
}

impl PlayerCookieResponseEvent {
    #[must_use]
    pub const fn new(player: Arc<Player>, key: String, payload: Option<Vec<u8>>) -> Self {
        Self {
            player,
            key,
            payload,
        }
    }
}

impl PlayerEvent for PlayerCookieResponseEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
