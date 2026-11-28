#[allow(clippy::wildcard_imports)]
use super::*;
use crate::block::entities::beacon::BeaconBlockEntity;
use pumpkin_inventory::Inventory;
use pumpkin_inventory::beacon_screen_handler::BeaconScreenHandler;
use pumpkin_protocol::java::server::play::SSetBeacon;

impl JavaClient {
    #[expect(clippy::too_many_lines)]
    pub fn handle_set_beacon(&self, player: &Arc<Player>, packet: &SSetBeacon) {
        let is_valid = {
            let screen_handler_lock = player
                .current_screen_handler
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            let mut screen_handler = screen_handler_lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let Some(beacon_handler) = screen_handler
                .as_any_mut()
                .downcast_mut::<BeaconScreenHandler>()
            else {
                debug!(
                    "Player {} interacted with invalid menu (expected beacon)",
                    player.gameprofile.name
                );
                return;
            };

            let Some(beacon_entity) = beacon_handler
                .inventory
                .as_any()
                .downcast_ref::<BeaconBlockEntity>()
            else {
                return;
            };

            // Check if payment slot has an item
            if beacon_entity
                .payment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
            {
                return;
            }

            let levels = beacon_entity.levels.load(Ordering::Relaxed);
            let primary_id = packet.primary_effect.map(|v| v.0);
            let secondary_id = packet.secondary_effect.map(|v| v.0);

            if BeaconBlockEntity::validate_effects(primary_id, secondary_id, levels) {
                // Paper `PlayerChangeBeaconEffectEvent`
                let effect_name = |id: Option<i32>| {
                    id.and_then(|id| u16::try_from(id).ok())
                        .and_then(<pumpkin_data::effect::StatusEffect as pumpkin_data::data_component_impl::IDSetContent>::from_id)
                        .map(|effect| effect.minecraft_name.to_string())
                };
                let mut consume = true;
                let (mut primary_id, mut secondary_id) = (primary_id, secondary_id);
                if let Some(event) = player.plugin_event(|player| {
                    crate::plugin::api::events::player::player_change_beacon_effect::PlayerChangeBeaconEffectEvent {
                        player,
                        beacon: beacon_entity.position,
                        primary: effect_name(primary_id),
                        secondary: effect_name(secondary_id),
                        consume_item: true,
                        cancelled: false,
                    }
                }) {
                    if event.cancelled {
                        return;
                    }
                    let effect_id = |name: Option<String>| {
                        name.and_then(|name| {
                            pumpkin_data::effect::StatusEffect::from_minecraft_name(&name)
                                .or_else(|| pumpkin_data::effect::StatusEffect::from_name(&name))
                        })
                        .map(|effect| i32::from(effect.id))
                    };
                    primary_id = effect_id(event.primary);
                    secondary_id = effect_id(event.secondary);
                    consume = event.consume_item;
                }
                beacon_entity
                    .primary_effect
                    .store(primary_id.unwrap_or(-1), Ordering::Relaxed);
                beacon_entity
                    .secondary_effect
                    .store(secondary_id.unwrap_or(-1), Ordering::Relaxed);

                // Remove 1 item from payment slot
                if consume {
                    beacon_entity.remove_stack_specific(0, 1);
                }
                beacon_entity.mark_dirty();

                screen_handler.sync_state();

                info!(
                    "Player {} updated beacon effects: primary {:?}, secondary {:?}",
                    player.gameprofile.name, primary_id, secondary_id
                );
                true
            } else {
                false
            }
        };

        if !is_valid {
            let primary_id = packet.primary_effect.map(|v| v.0);
            let secondary_id = packet.secondary_effect.map(|v| v.0);
            warn!(
                "Player {} tried to set invalid beacon effects: primary {:?}, secondary {:?}",
                player.gameprofile.name, primary_id, secondary_id
            );
            self.try_kick(&TextComponent::translate(
                "multiplayer.disconnect.generic",
                &[],
            ));
        }
    }
}
