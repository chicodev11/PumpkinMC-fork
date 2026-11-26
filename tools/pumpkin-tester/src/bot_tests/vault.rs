use std::sync::Arc;

use pumpkin_core::block::entities::vault::VaultBlockEntity;
use pumpkin_core::entity::EntityBase;
use pumpkin_core::server::Server;
use pumpkin_core::world::BlockFlags;
use pumpkin_data::Block;
use pumpkin_data::block_properties::{VaultLikeProperties, VaultState};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;

use crate::bot::HeadlessBot;

fn state(world: &pumpkin_core::world::World, pos: &BlockPos) -> VaultState {
    VaultLikeProperties::from_state_id(world.get_block_state_id(pos)).vault_state
}

/// A vault near a player activates, refuses a wrong key, takes one trial key, ejects its
/// reward, goes inactive for the rewarded player and refuses a second key.
pub fn run_vault_test(
    server: &Arc<Server>,
    bot: &HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let player = server.get_player_by_name(&bot.name).ok_or("missing bot")?;
    let original_position = player.position();
    let original_mode = player.gamemode.load();
    let result = run_inner(server, &player, summary_lines);
    player.get_entity().set_pos(original_position);
    player.set_gamemode(original_mode);
    result
}

fn run_inner(
    server: &Arc<Server>,
    player: &Arc<pumpkin_core::entity::player::Player>,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let mut log = Vec::new();
    let out = &mut log;
    player.set_gamemode(GameMode::Survival);
    let world = player.world();
    let feet = player.get_entity().block_pos.load();
    let base = BlockPos::new(feet.0.x, feet.0.y + 20, feet.0.z);
    // Floor for the player and the vault next to it.
    for dx in -2..=2 {
        for dz in -2..=2 {
            world.set_block_state(
                &BlockPos::new(base.0.x + dx, base.0.y - 1, base.0.z + dz),
                Block::STONE.default_state.id,
                BlockFlags::NOTIFY_ALL,
            );
        }
    }
    let vault_pos = BlockPos::new(base.0.x + 2, base.0.y, base.0.z);
    world.set_block_state(
        &vault_pos,
        VaultLikeProperties::default(&Block::VAULT).to_state_id(&Block::VAULT),
        BlockFlags::NOTIFY_ALL,
    );
    world.add_block_entity(Arc::new(VaultBlockEntity::new(vault_pos)));
    player
        .get_entity()
        .set_pos(base.to_centered_f64().add_raw(0.0, -0.5, 0.0));

    for _ in 0..45 {
        server.tick();
    }
    let active = state(&world, &vault_pos);
    out.push(format!("vault after 45 ticks near a player: {active:?}"));
    let entity = world
        .get_block_entity(&vault_pos)
        .ok_or("no vault entity")?;
    let vault = entity
        .as_any()
        .downcast_ref::<VaultBlockEntity>()
        .ok_or("not a vault")?;
    let display = vault.lock().shared.display_item.item.registry_key;
    out.push(format!("display item: {display}"));
    if active != VaultState::Active {
        return Err(format!(
            "vault did not activate: {active:?} ({})",
            log.join("; ")
        ));
    }

    let mut wrong = ItemStack::new(1, &Item::DIAMOND);
    vault.try_insert_key(&world, player, &mut wrong);
    out.push(format!(
        "wrong key: count {} state {:?}",
        wrong.item_count,
        state(&world, &vault_pos)
    ));

    let mut key = ItemStack::new(2, &Item::TRIAL_KEY);
    vault.try_insert_key(&world, player, &mut key);
    out.push(format!(
        "after key: keys left {} state {:?}",
        key.item_count,
        state(&world, &vault_pos)
    ));
    let mut states = Vec::new();
    for _ in 0..200 {
        server.tick();
        let now = state(&world, &vault_pos);
        if states.last() != Some(&now) {
            states.push(now);
        }
    }
    let items = world
        .entities
        .load()
        .iter()
        .filter(|e| {
            e.get_entity().entity_type == &pumpkin_data::entity::EntityType::ITEM
                && e.get_entity()
                    .pos
                    .load()
                    .squared_distance_to_vec(&vault_pos.to_centered_f64())
                    < 16.0
        })
        .count();
    out.push(format!("states: {states:?}, items near: {items}"));
    out.push(format!(
        "rewarded: {}",
        vault.has_rewarded(&player.gameprofile.id)
    ));
    let mut again = ItemStack::new(1, &Item::TRIAL_KEY);
    vault.try_insert_key(&world, player, &mut again);
    out.push(format!("second key kept: {}", again.item_count));
    if items == 0 || !vault.has_rewarded(&player.gameprofile.id) || again.item_count != 1 {
        return Err(format!("vault reward flow differs: {}", log.join("; ")));
    }
    summary_lines.push("  vault: activate, key, eject, inactive, second key refused".into());
    Ok(())
}
