use std::sync::Arc;

use pumpkin_macros::pumpkin_block;

use crate::block::entities::end_gateway::EndGatewayBlockEntity;
use crate::block::{BlockBehaviour, OnEntityCollisionArgs, OnSyncedBlockEventArgs, PlacedArgs};

#[pumpkin_block("minecraft:end_gateway")]
pub struct EndGatewayBlock;

impl BlockBehaviour for EndGatewayBlock {
    fn on_entity_collision(&self, args: OnEntityCollisionArgs<'_>) {
        let entity = args.entity.get_entity();
        if entity
            .portal_cooldown
            .load(std::sync::atomic::Ordering::Relaxed)
            > 0
        {
            return;
        }

        let Some(block_entity) = args.world.get_block_entity(args.position) else {
            return;
        };
        let Some(end_gateway) = block_entity
            .as_any()
            .downcast_ref::<EndGatewayBlockEntity>()
        else {
            return;
        };

        if end_gateway.is_cooling_down() {
            return;
        }

        end_gateway.trigger_cooldown(args.world, *args.position);

        if let Some(mut destination) = end_gateway.get_portal_position(args.world, *args.position) {
            let yaw = entity.yaw.load();
            let pitch = entity.pitch.load();
            // Paper `PlayerTeleportEndGatewayEvent` / `EntityTeleportEndGatewayEvent`
            let from = entity.pos.load();
            if let Some(player) = args.entity.get_player() {
                match player.plugin_event(|player| {
                    crate::plugin::api::events::player::player_teleport_end_gateway::PlayerTeleportEndGatewayEvent {
                        player,
                        from,
                        to: destination,
                        gateway: *args.position,
                        cancelled: false,
                    }
                }) {
                    Some(event) if event.cancelled => return,
                    Some(event) => destination = event.to,
                    None => {}
                }
            } else {
                match args.world.entity_event(entity.entity_id, |teleported| {
                    crate::plugin::api::events::entity::entity_teleport_end_gateway::EntityTeleportEndGatewayEvent {
                        entity: teleported,
                        from,
                        to: destination,
                        gateway: *args.position,
                        cancelled: false,
                    }
                }) {
                    Some(event) if event.cancelled => return,
                    Some(event) => destination = event.to,
                    None => {}
                }
            }
            args.entity
                .teleport(destination, Some(yaw), Some(pitch), args.world.clone());
        }
    }

    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(end_gateway) = block_entity
                .as_any()
                .downcast_ref::<EndGatewayBlockEntity>()
        {
            end_gateway.trigger_event(args.r#type, args.data)
        } else {
            args.r#type == EndGatewayBlockEntity::EVENT_COOLDOWN
        }
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        let block_entity = EndGatewayBlockEntity::new(*args.position);
        args.world.add_block_entity(Arc::new(block_entity));
    }
}
