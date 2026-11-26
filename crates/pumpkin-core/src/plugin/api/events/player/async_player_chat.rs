use pumpkin_macros::{Event, cancellable};
use pumpkin_util::text::TextComponent;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// An asynchronous event that occurs when a player sends a chat message.
#[cancellable]
#[derive(Event, Clone)]
pub struct AsyncPlayerChatEvent {
    /// The player sending the message.
    pub player: Arc<Player>,

    /// The chat message content.
    pub message: String,

    /// The formatted chat message.
    pub format: TextComponent,
}

impl PlayerEvent for AsyncPlayerChatEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

impl AsyncPlayerChatEvent {
    /// Paper `AsyncPlayerChatEvent`: plugins may cancel the line, change the message or
    /// replace `format`, the whole decorated chat line. When only the message changed, the
    /// line is decorated again with `redecorate`. Returns `(message, line)` or `None` when
    /// cancelled.
    pub async fn fire(
        server: &Arc<crate::server::Server>,
        player: &Arc<Player>,
        message: String,
        line: TextComponent,
        redecorate: impl Fn(&str) -> TextComponent,
    ) -> Option<(String, TextComponent)> {
        if !server.plugin_manager.has_handlers::<Self>() {
            return Some((message, line));
        }
        let mut event = Self {
            player: player.clone(),
            message: message.clone(),
            format: line.clone(),
            cancelled: false,
        };
        server.plugin_manager.fire(server, &mut event).await;
        if event.cancelled {
            return None;
        }
        let line = if event.format == line && event.message != message {
            redecorate(&event.message)
        } else {
            event.format
        };
        Some((event.message, line))
    }
}
