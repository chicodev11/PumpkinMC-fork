#[allow(clippy::wildcard_imports)]
use super::*;

impl JavaClient {
    pub fn handle_swing_arm(
        &self,
        server: &Arc<Server>,
        player: &Arc<Player>,
        swing_arm: &SSwingArm,
    ) {
        player.update_last_action_time();
        let Ok(hand) = Hand::from_packet_id(swing_arm.hand.0) else {
            self.try_kick(&TextComponent::text("Invalid hand"));
            return;
        };

        // Paper `PlayerArmSwingEvent` (a `PlayerAnimationEvent` there)
        if player
            .plugin_event(|player| {
                crate::plugin::api::events::player::player_arm_swing::PlayerArmSwingEvent {
                    player,
                    hand: if hand == Hand::Left {
                        "OFF_HAND"
                    } else {
                        "HAND"
                    }
                    .to_string(),
                    cancelled: false,
                }
            })
            .is_some_and(|event| event.cancelled)
        {
            return;
        }

        let mut anim_event = crate::plugin::api::events::player::player_animation::PlayerAnimationEvent::new(
            player.clone(),
            match hand {
                Hand::Left => crate::plugin::api::events::player::player_animation::PlayerAnimationType::ArmSwingOff,
                Hand::Right => crate::plugin::api::events::player::player_animation::PlayerAnimationType::ArmSwingMain,
            },
        );
        server.plugin_manager.fire_blocking(server, &mut anim_event);
        if anim_event.cancelled {
            return;
        }

        let (yaw, pitch) = player.rotation();
        let hit = player.world().clip(&pumpkin_world::clip::ClipContext::new(
            player.eye_position(),
            player
                .eye_position()
                .add(&(Vector3::rotation_vector(f64::from(pitch), f64::from(yaw)) * 4.5)),
            pumpkin_world::clip::ClipBlock::Outline,
            pumpkin_world::clip::ClipFluid::None,
        ));

        // The client also swings after a used item, an entity hit or a drop. Like Paper, only a
        // main hand swing that none of those caused is a left click in the air.
        if hit.is_miss && hand == Hand::Right && !player.swing_follows_action() {
            let event =
                PlayerInteractEvent::new(player, InteractAction::LeftClickAir, &Block::AIR, None);
            send_cancellable_blocking! {{
                &server;
                event;
                'after: {
                    player.swing_hand(hand, false);
                }
            }}
        } else {
            player.swing_hand(hand, false);
        }
    }
}
