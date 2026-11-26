use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// An event that occurs before a player command is executed.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerCommandPreprocessEvent {
    /// The player sending the command.
    pub player: Arc<Player>,

    /// The raw command string.
    pub command: String,
}

impl PlayerEvent for PlayerCommandPreprocessEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

impl PlayerCommandPreprocessEvent {
    /// Paper `PlayerCommandPreprocessEvent`: fired with the message as typed (leading `/`)
    /// before anything else handles a player's command. Returns the command to run without
    /// its leading `/`, or `None` when a plugin cancelled it.
    pub async fn fire(
        server: &Arc<crate::server::Server>,
        player: &Arc<Player>,
        command: &str,
    ) -> Option<String> {
        let mut event = Self {
            player: player.clone(),
            command: format!("/{command}"),
            cancelled: false,
        };
        server.plugin_manager.fire(server, &mut event).await;
        if event.cancelled {
            return None;
        }
        Some(
            event
                .command
                .strip_prefix('/')
                .map_or_else(|| event.command.clone(), str::to_string),
        )
    }
}
