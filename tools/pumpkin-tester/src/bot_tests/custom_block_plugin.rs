//! The `addon-smoke` WASM plugin (built for wasm32-wasip2, see `official-plugins/addon-smoke`)
//! registers `addon_smoke:test_block` with an `on-use` callback (puts a gold block on top) and
//! a `drops` callback (a diamond). This runs the block through the real plugin host.

use std::sync::Arc;

use pumpkin_core::entity::EntityBase;
use pumpkin_core::server::Server;
use pumpkin_core::world::BlockFlags;
use pumpkin_data::{Block, BlockState, custom_block};
use pumpkin_util::GameMode;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::bot::HeadlessBot;

const BLOCK: &str = "addon_smoke:test_block";

pub async fn run_custom_block_plugin_test(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    if !crate::harness::smoke_plugin_loaded() {
        summary_lines.push(
            "[SKIP] bot:custom_block_plugin (build official-plugins/addon-smoke for wasm32-wasip2)"
                .to_string(),
        );
        return Ok(());
    }
    if !server.plugin_manager.is_plugin_active("addon-smoke") {
        return Err("addon-smoke was loaded but is not active".to_string());
    }
    let player = server.get_player_by_name(&bot.name).ok_or("missing bot")?;
    let original_position = player.position();
    let original_mode = player.gamemode.load();
    let result = run_inner(server, bot, &player).await;
    player.get_entity().set_pos(original_position);
    player.set_gamemode(original_mode);
    result?;
    summary_lines.push(
        "[PASS] bot:custom_block_plugin (register, use and drops callbacks, nested event, death drops)"
            .to_string(),
    );
    Ok(())
}

async fn tick(server: &Arc<Server>, count: usize) {
    for _ in 0..count {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        server.tick();
    }
}

async fn run_inner(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<pumpkin_core::entity::player::Player>,
) -> Result<(), String> {
    let state = custom_block::resolve_state(BLOCK, [])
        .ok_or_else(|| format!("{BLOCK} is not registered"))?;
    if custom_block::client_state(state) != Block::STONE.default_state.id {
        return Err(format!("{BLOCK} is not shown to clients as stone"));
    }
    let hardness = BlockState::from_id(state).hardness;
    if (hardness - 1.5).abs() > f32::EPSILON {
        return Err(format!("{BLOCK} has hardness {hardness}, expected 1.5"));
    }

    player.set_gamemode(GameMode::Survival);
    let world = player.world();
    let feet = player.get_entity().block_pos.load();
    let base = BlockPos::new(feet.0.x, feet.0.y + 20, feet.0.z);
    for dx in -2..=2 {
        for dz in -2..=2 {
            world.set_block_state(
                &BlockPos::new(base.0.x + dx, base.0.y - 1, base.0.z + dz),
                Block::STONE.default_state.id,
                BlockFlags::NOTIFY_ALL,
            );
        }
    }
    player.get_entity().set_pos(Vector3::new(
        f64::from(base.0.x) + 0.5,
        f64::from(base.0.y),
        f64::from(base.0.z) + 0.5,
    ));
    let pos = BlockPos::new(base.0.x + 2, base.0.y, base.0.z);
    let above = pos.up();
    world.set_block_state(&above, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    world.set_block_state(&pos, state, BlockFlags::NOTIFY_ALL);
    tick(server, 2).await;
    if world.get_block_state_id(&pos) != state {
        return Err(format!("{BLOCK} did not stay in the world"));
    }

    // Right click through the network: the plugin's on-use callback spawns an entity (its own
    // spawn handler must run inside that call) and places gold on top.
    bot.use_block(pos, 1200).await?;
    for _ in 0..40 {
        tick(server, 1).await;
        if world.get_block(&above).id == Block::GOLD_BLOCK.id {
            break;
        }
    }
    if world.get_block(&above).id == Block::REDSTONE_BLOCK.id {
        return Err(
            "the plugin's spawn event did not run inside its own spawn-entity call (nested event order)"
                .to_string(),
        );
    }
    if world.get_block(&above).id != Block::GOLD_BLOCK.id {
        return Err("the on-use callback did not place a gold block".to_string());
    }

    // Breaking it drops what the drops callback returned. Items left here by earlier tests
    // (the vault's reward) go first.
    world.set_block_state(&above, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    let area = BoundingBox::new(
        Vector3::new(
            f64::from(pos.0.x) - 1.0,
            f64::from(pos.0.y) - 1.0,
            f64::from(pos.0.z) - 1.0,
        ),
        Vector3::new(
            f64::from(pos.0.x) + 2.0,
            f64::from(pos.0.y) + 2.0,
            f64::from(pos.0.z) + 2.0,
        ),
    );
    for entity in world.get_entities_at_box(&area) {
        if entity.get_item_entity().is_some() {
            entity.get_entity().remove();
        }
    }
    tick(server, 1).await;
    world.break_block_naturally(&pos, None, false, BlockFlags::NOTIFY_ALL);
    tick(server, 2).await;
    let mut dropped = Vec::new();
    for entity in world.get_entities_at_box(&area) {
        if let Some(item) = entity.get_item_entity() {
            let stack = item
                .get_item_stack()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            dropped.push(stack.item.registry_key.to_string());
            entity.get_entity().remove();
        }
    }
    if dropped != ["diamond"] {
        return Err(format!(
            "the drops callback gave {dropped:?}, expected one diamond"
        ));
    }
    // MythicMobs-style drops: the plugin's death handler replaces a tagged zombie's drops.
    let zombie = pumpkin_core::entity::r#type::from_type(
        &pumpkin_data::entity::EntityType::ZOMBIE,
        pos.to_centered_f64(),
        &world,
        uuid::Uuid::new_v4(),
    );
    zombie.get_entity().add_scoreboard_tag("addon_smoke_drops");
    world.spawn_entity(zombie.clone());
    tick(server, 1).await;
    zombie.kill(zombie.as_ref());
    let mut emeralds = 0;
    for _ in 0..40 {
        tick(server, 1).await;
        for entity in world.get_entities_at_box(&area) {
            if let Some(item) = entity.get_item_entity() {
                let stack = item
                    .get_item_stack()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                if stack.item.registry_key == "emerald" {
                    emeralds += stack.item_count;
                } else {
                    return Err(format!(
                        "the death handler left {} in the zombie's drops",
                        stack.item.registry_key
                    ));
                }
                entity.get_entity().remove();
            }
        }
        if emeralds > 0 {
            break;
        }
    }
    if emeralds != 3 {
        return Err(format!(
            "the zombie dropped {emeralds} emeralds, expected 3"
        ));
    }
    Ok(())
}
