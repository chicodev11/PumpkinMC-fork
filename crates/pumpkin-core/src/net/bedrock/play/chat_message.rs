#[allow(clippy::wildcard_imports)]
use super::*;

impl BedrockClient {
    pub async fn handle_chat_message(
        &self,
        server: &Arc<Server>,
        player: &Arc<Player>,
        packet: SText<'_>,
    ) {
        player.update_last_action_time();
        if player.check_chat_spam(server, crate::entity::player::SpamType::Chat) {
            return;
        }
        let gameprofile = &player.gameprofile;

        send_cancellable! {{
            server;
            PlayerChatEvent::new(player.clone(), packet.message.into_owned(), vec![], None);

            'after: {
                info!("<chat> {}: {}", gameprofile.name, event.message);

                let config = &server.advanced_config;

                let message = match seasonal_events::modify_chat_message(&event.message, config) {
                    Some(m) => m,
                    None => event.message.clone(),
                };

                let decorated_message = crate::net::chat::decorate_chat(player, &config.chat.format, &message);
                let Some((message, decorated_message)) =
                    crate::plugin::api::events::player::async_player_chat::AsyncPlayerChatEvent::fire(
                        server,
                        player,
                        message,
                        decorated_message,
                        |message| crate::net::chat::decorate_chat(player, &config.chat.format, message),
                    )
                    .await
                else {
                    return;
                };

                let entity = &player.get_entity();
                // Bedrock messages are never signed, so they are system chat even with
                // chat reports on (Java secure chat needs a signature).
                let je_packet = CSystemChatMessage::new(
                    &decorated_message,
                    false,
                );

                let be_packet = SText::chat_with_xuid(
                    message,
                    gameprofile.name.clone(),
                    packet.xuid.into_owned(),
                    packet.filtered_message.map(std::borrow::Cow::into_owned),
                );

                entity.world.load().broadcast_editioned(&je_packet, &be_packet);
            }
        }}
    }
}
