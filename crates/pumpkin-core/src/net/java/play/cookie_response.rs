#[allow(clippy::wildcard_imports)]
use super::*;

impl JavaClient {
    /// Hands a cookie the client sent back (after `Player::request_cookie`) to plugins.
    pub fn handle_cookie_response(&self, packet: &SPCookieResponse<'_>) {
        debug!(
            "Received cookie_response[play]: key: \"{}\", payload_length: \"{:?}\"",
            packet.key,
            packet.payload.as_ref().map(|p| p.len())
        );
        let Some(player) = self.player.load_full().as_ref().clone() else {
            return;
        };
        let world = player.world();
        world.plugin_event(|| {
            crate::plugin::api::events::player::player_cookie_response::PlayerCookieResponseEvent::new(
                player.clone(),
                packet.key.to_string(),
                packet.payload.map(<[u8]>::to_vec),
            )
        });
    }
}
