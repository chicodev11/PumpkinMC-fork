#[allow(clippy::wildcard_imports)]
use super::*;
use pumpkin_data::tag::Taggable;

impl JavaClient {
    pub fn handle_pick_item_from_block(
        &self,
        player: &Arc<Player>,
        pick_item: &SPickItemFromBlock,
    ) {
        if !player.can_interact_with_block_at(&pick_item.pos, 1.0) {
            return;
        }

        let world = player.world();
        let block = world.get_block(&pick_item.pos);
        let state_id = world.get_block_state_id(&pick_item.pos);

        // Custom blocks pick their custom item when the plugin named one.
        let default_stack = if block.is_custom() {
            pumpkin_data::custom_item::get(block.name).or_else(|| {
                Item::from_id(block.item_id)
                    .filter(|_| block.item_id != 0)
                    .map(|item| ItemStack::new(1, item))
            })
        } else if block.item_id == 0 {
            // Invalid block id (blocks such as tall seagrass)
            None
        } else {
            Item::from_id(block.item_id).map(|item| ItemStack::new(1, item))
        };

        // Plugins may replace (or cancel) the pick.
        let mut stack = default_stack.unwrap_or_else(|| ItemStack::EMPTY.clone());
        if let Some(server) = world.server.upgrade() {
            let mut event =
                crate::plugin::api::events::block::player_pick_block::PlayerPickBlockEvent::new(
                    player.clone(),
                    world.clone(),
                    pick_item.pos,
                    state_id,
                    stack,
                    pick_item.include_data,
                );
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return;
            }
            stack = event.item;
        }
        if stack.is_empty() {
            return;
        }

        if block.has_tag(&pumpkin_data::tag::Block::MINECRAFT_COPPER_GOLEM_STATUES)
            && let Some(block_entity) = world.get_block_entity(&pick_item.pos)
            && let Some(statue) = block_entity.as_any().downcast_ref::<
                crate::block::entities::copper_golem_statue::CopperGolemStatueBlockEntity,
            >()
        {
            let props = pumpkin_data::block_properties::CopperGolemStatueLikeProperties::from_state_id(
                world.get_block_state_id(&pick_item.pos),
            );
            statue.get_item(&mut stack, props.copper_golem_pose);
        }
        // Vanilla `addCustomNbtData` for a creative pick with data.
        if player.gamemode.load() == GameMode::Creative
            && pick_item.include_data
            && let Some(block_entity) = world.get_block_entity(&pick_item.pos)
        {
            let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
            block_entity.write_nbt(&mut nbt);
            if !nbt.is_empty() {
                nbt.put_string("id", block_entity.resource_location().to_string());
                stack.set_data_component(pumpkin_data::data_component_impl::BlockEntityDataImpl {
                    nbt,
                });
            }
            block_entity.collect_pick_item_components(&mut stack);
        }

        let slot_with_stack = player.inventory().get_slot_with_stack(&stack);

        if slot_with_stack != -1 {
            if PlayerInventory::is_valid_hotbar_index(slot_with_stack as usize) {
                player.inventory.set_selected_slot(slot_with_stack as u8);
            } else {
                player
                    .inventory
                    .swap_slot_with_hotbar(slot_with_stack as usize);
            }
        } else if player.gamemode.load() == GameMode::Creative {
            player.inventory.swap_stack_with_hotbar(stack);
        }

        player.try_send_client_packet(&CSetSelectedSlot::new(
            player.inventory.get_selected_slot() as i8
        ));
        player
            .player_screen_handler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send_content_updates();
    }

    pub fn handle_pick_item_from_entity(
        &self,
        player: &Arc<Player>,
        pick_item: &SPickItemFromEntity,
    ) {
        use pumpkin_data::entity::{entity_from_egg, spawn_egg_ids};

        let world = player.world();
        let Some(target) = world.get_entity_by_id(pick_item.id.0) else {
            return;
        };

        let p_eye = player.get_entity().get_eye_pos();
        let t_eye = target.get_eye_pos();
        let dx = p_eye.x - t_eye.x;
        let dy = p_eye.y - t_eye.y;
        let dz = p_eye.z - t_eye.z;
        if dx * dx + dy * dy + dz * dz > 64.0 {
            return;
        }

        let target_type_id = target.get_entity().entity_type.id;
        let mut found_egg: Option<u16> = None;
        for &egg_id in &spawn_egg_ids() {
            if let Some(et) = entity_from_egg(egg_id)
                && et.id == target_type_id
            {
                found_egg = Some(egg_id);
                break;
            }
        }

        // Plugins may replace (or cancel) the pick, also for entities without a spawn egg
        // (furniture hitboxes).
        let mut stack = found_egg
            .and_then(Item::from_id)
            .map_or_else(|| ItemStack::EMPTY.clone(), |item| ItemStack::new(1, item));
        if let Some(server) = world.server.upgrade() {
            let mut event =
                crate::plugin::api::events::player::player_pick_entity::PlayerPickEntityEvent::new(
                    player.clone(),
                    pick_item.id.0,
                    stack,
                    pick_item.include_data,
                );
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return;
            }
            stack = event.item;
        }

        if !stack.is_empty() {
            let slot_with_stack = player.inventory().get_slot_with_stack(&stack);

            if slot_with_stack != -1 {
                if PlayerInventory::is_valid_hotbar_index(slot_with_stack as usize) {
                    player.inventory.set_selected_slot(slot_with_stack as u8);
                } else {
                    player
                        .inventory
                        .swap_slot_with_hotbar(slot_with_stack as usize);
                }
            } else if player.gamemode.load() == GameMode::Creative {
                player.inventory.swap_stack_with_hotbar(stack);
            }

            player.try_send_client_packet(&CSetSelectedSlot::new(
                player.inventory.get_selected_slot() as i8,
            ));
            player
                .player_screen_handler
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .send_content_updates();
        }
    }
}
