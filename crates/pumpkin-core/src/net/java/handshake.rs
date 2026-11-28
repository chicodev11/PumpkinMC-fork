use pumpkin_data::{
    packet::{CURRENT_MC_VERSION, LOWEST_SUPPORTED_MC_VERSION},
    translation,
};
use pumpkin_protocol::{ConnectionState, java::server::handshake::SHandShake};
use pumpkin_util::{text::TextComponent, version::JavaMinecraftVersion};
use tracing::debug;

use crate::{net::java::pending::PendingConnection, server::Server};
use std::sync::Arc;

impl PendingConnection {
    pub async fn handle_handshake(&mut self, server: &Arc<Server>, handshake: SHandShake) {
        let version = handshake.protocol_version.0 as u32;
        self.server_address = handshake.server_address.to_string();
        self.server_port = handshake.server_port;
        self.version
            .store(JavaMinecraftVersion::from_protocol(version));

        debug!("Handshake: next state is {:?}", &handshake.next_state);
        self.connection_state.store(handshake.next_state);
        // Paper `PlayerHandshakeEvent`: a plugin handling the handshake (it stays cancelled
        // otherwise) may rewrite the server address (for example to read proxy data) or refuse it
        if handshake.next_state != ConnectionState::Status {
            use crate::plugin::api::events::player::player_handshake::PlayerHandshakeEvent;
            if server.plugin_manager.has_handlers::<PlayerHandshakeEvent>() {
                let mut event = PlayerHandshakeEvent {
                    original_handshake: self.server_address.clone(),
                    server_hostname: self.server_address.clone(),
                    failed: false,
                    fail_message: "If you wish to use IP forwarding, please enable it in your BungeeCord config as well!".to_string(),
                    cancelled: true,
                };
                server.plugin_manager.fire(server, &mut event).await;
                if !event.cancelled {
                    if event.failed {
                        self.kick(TextComponent::text(event.fail_message)).await;
                        return;
                    }
                    self.server_address = event.server_hostname;
                }
            }
        }
        if handshake.next_state == ConnectionState::Transfer
            && !server.basic_config.accepts_transfers
        {
            self.kick(TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_TRANSFERS_DISABLED,
                translation::java::MULTIPLAYER_DISCONNECT_TRANSFERS_DISABLED,
                [],
            ))
            .await;
            return;
        }
        if self.connection_state.load() != ConnectionState::Status {
            let protocol = version;
            // A plugin translating this version (see `register_java_versions`) admits it.
            if server
                .plugin_manager
                .java_versions
                .admits_protocol(protocol)
            {
                return;
            }
            if protocol < LOWEST_SUPPORTED_MC_VERSION.protocol_version() as u32 {
                self.kick(TextComponent::translate_cross(
                    translation::java::MULTIPLAYER_DISCONNECT_OUTDATED_CLIENT,
                    translation::bedrock::DISCONNECTIONSCREEN_OUTDATEDCLIENT,
                    [TextComponent::text(CURRENT_MC_VERSION.to_string())],
                ))
                .await;
            } else if protocol > CURRENT_MC_VERSION.protocol_version() as u32 {
                self.kick(TextComponent::translate_cross(
                    translation::java::MULTIPLAYER_DISCONNECT_OUTDATED_SERVER,
                    translation::bedrock::DISCONNECTIONSCREEN_OUTDATEDSERVER,
                    [TextComponent::text(CURRENT_MC_VERSION.to_string())],
                ))
                .await;
            }
        }
    }
}
