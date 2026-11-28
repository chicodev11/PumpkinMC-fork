use pumpkin_macros::Event;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// Paper `PlayerClientOptionsChangeEvent`: the client sent its options, before they apply. Read-only.
#[derive(Event, Clone)]
pub struct PlayerClientOptionsChangeEvent {
    pub player: Arc<Player>,
    pub locale: String,
    pub view_distance: i32,
    pub chat_visibility: String,
    pub chat_colors: bool,
    pub skin_parts: i32,
    pub main_hand: String,
    pub text_filtering: bool,
    pub allow_server_listings: bool,
}

impl PlayerEvent for PlayerClientOptionsChangeEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
