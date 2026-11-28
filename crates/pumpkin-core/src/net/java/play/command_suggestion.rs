#[allow(clippy::wildcard_imports)]
use super::*;

impl JavaClient {
    pub fn handle_command_suggestion(
        &self,
        player: &Arc<Player>,
        packet: &SCommandSuggestion<'_>,
        server: &Arc<Server>,
    ) {
        let Some(cmd) = &packet.command.get(1..) else {
            return;
        };

        let Some((last_word_start, _)) = cmd.char_indices().rfind(|(_, c)| c.is_whitespace())
        else {
            return;
        };

        let buffer = packet.command.to_string();
        let names = |suggestions: &[pumpkin_protocol::java::client::play::CommandSuggestion]| {
            suggestions
                .iter()
                .map(|suggestion| suggestion.suggestion.clone())
                .collect::<Vec<_>>()
        };
        let from_names = |names: Vec<String>| {
            names
                .into_iter()
                .map(|name| {
                    pumpkin_protocol::java::client::play::CommandSuggestion::new(name, None)
                })
                .collect::<Vec<_>>()
        };

        // Paper `AsyncTabCompleteEvent`: a handled event replaces the dispatcher's suggestions
        let mut suggestions = None;
        if let Some(event) = player.plugin_event(|player| {
            crate::plugin::api::events::server::async_tab_complete::AsyncTabCompleteEvent {
                player,
                buffer: buffer.clone(),
                completions: Vec::new(),
                is_command: true,
                handled: false,
                cancelled: false,
            }
        }) {
            if event.cancelled {
                return;
            }
            if event.handled {
                suggestions = Some(from_names(event.completions));
            }
        }
        let mut suggestions = suggestions.unwrap_or_else(|| {
            server
                .command_dispatcher
                .load()
                .suggest(cmd, &player.get_command_source(server))
        });

        // Paper `TabCompleteEvent`
        if let Some(event) = player.plugin_event(|player| {
            crate::plugin::api::events::server::tab_complete::TabCompleteEvent {
                player,
                buffer: buffer.clone(),
                completions: names(&suggestions),
                cancelled: false,
            }
        }) {
            if event.cancelled {
                return;
            }
            if event.completions != names(&suggestions) {
                suggestions = from_names(event.completions);
            }
        }

        // Paper `AsyncPlayerSendSuggestionsEvent`
        if let Some(event) = player.plugin_event(|player| {
            crate::plugin::api::events::player::async_player_send_suggestions::AsyncPlayerSendSuggestionsEvent {
                player,
                buffer: buffer.clone(),
                suggestions: names(&suggestions),
                cancelled: false,
            }
        }) {
            if event.cancelled {
                return;
            }
            if event.suggestions != names(&suggestions) {
                suggestions = from_names(event.suggestions);
            }
        }

        let response = CCommandSuggestions::new(
            packet.id,
            ((last_word_start + 2) as i32).into(),
            ((cmd.len() - last_word_start - 1) as i32).into(),
            suggestions.into(),
        );

        player.try_send_client_packet(&response);
    }
}
