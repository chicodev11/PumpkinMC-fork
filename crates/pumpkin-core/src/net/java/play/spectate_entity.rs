#[allow(clippy::wildcard_imports)]
use super::*;
use pumpkin_protocol::java::server::play::SSpectateEntity;
use pumpkin_util::GameMode;

impl JavaClient {
    pub fn handle_spectate_entity(&self, player: &Arc<Player>, packet: &SSpectateEntity) {
        if !player.has_client_loaded() {
            return;
        }
        player.update_last_action_time();

        if player.gamemode.load() != GameMode::Spectator {
            return;
        }

        let Some(target_id) = packet.target_entity_id() else {
            return;
        };

        let world = player.world();
        if let Some(target) = world.get_entity_by_id(target_id) {
            player.set_camera_entity_id(target.get_entity().entity_id);
        }
    }
}
