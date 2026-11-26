mod block_tuning_and_invert;
mod client_item_view;
mod commands_and_gamemode;
mod embedded_stop;
mod login_and_movement;
mod loot_tables;
mod player_actions;
mod polar_bear;
mod redstone_compare;
mod redstone_input_timing;
mod sheep_shearing;
mod sign_authorization;
mod snow_golem;
mod status_ping;
mod vault;
mod world_interaction;

use std::{net::SocketAddr, sync::Arc};

use pumpkin_core::server::Server;
use pumpkin_gametest::GameTestRunner;

pub async fn run_bot_tests(
    server: &Arc<Server>,
    addr: SocketAddr,
    runner: &mut GameTestRunner,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    status_ping::run_bot_status_ping_test(addr, summary_lines).await?;

    let (mut bot1, bot2) =
        login_and_movement::run_login_and_movement_tests(server, addr, summary_lines).await?;

    // `tools/redstone-compare`: trace the scenarios for a diff against vanilla, then stop.
    if let (Ok(scenarios), Ok(trace_out)) = (
        std::env::var("REDSTONE_COMPARE"),
        std::env::var("TRACE_OUT"),
    ) {
        redstone_compare::run(server, &scenarios, &trace_out)?;
        drop((bot1, bot2));
        std::process::exit(0);
    }

    commands_and_gamemode::run_commands_and_gamemode_tests(
        server,
        &mut bot1,
        runner,
        summary_lines,
    )
    .await?;

    loot_tables::run_loot_table_tests(server, &mut bot1, summary_lines).await?;

    world_interaction::run_world_interaction_tests(server, &mut bot1, summary_lines).await?;

    sheep_shearing::run_sheep_shearing_test(server, summary_lines)?;
    snow_golem::run_snow_golem_shearing_test(server, summary_lines)?;

    polar_bear::run_polar_bear_defense_test(server, &bot1, summary_lines)?;

    vault::run_vault_test(server, &bot1, summary_lines)?;

    client_item_view::run_client_item_view_test(server, &bot1.name, summary_lines)?;

    block_tuning_and_invert::run_block_tuning_and_invert_tests(server, &mut bot1, summary_lines)
        .await?;

    redstone_input_timing::run_redstone_input_timing_test(server, &mut bot1, summary_lines).await?;

    allay::run_allay_interaction_test(server, &mut bot1, summary_lines).await?;

    player_actions::run_player_actions_and_disconnect_tests(server, bot1, bot2, summary_lines)
        .await?;

    sign_authorization::run_sign_authorization_tests(server, addr, summary_lines).await?;

    embedded_stop::run_embedded_stop_test(summary_lines).await?;

    Ok(())
}
mod allay;
