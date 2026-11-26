use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use pumpkin_core::{server::Server, world::BlockFlags};
use pumpkin_data::{Block, block_properties::OakTrapdoorLikeProperties, dimension::Dimension};
use pumpkin_util::math::position::BlockPos;

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

pub async fn run_world_interaction_tests(
    server: &Arc<Server>,
    bot1: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let world = server.get_world_from_dimension(&Dimension::OVERWORLD);

    // The server's position, not the bot's client-side one: the reach check for the
    // right-click runs against the server's, and the two can differ until the bot's first
    // movement is accepted.
    let Some(player1) = world.get_player_by_name("TestBot1") else {
        return Err("Could not find 'TestBot1' in Overworld".to_string());
    };
    let server_pos = player1.position();
    let base_x = server_pos.x.floor() as i32;
    let base_y = server_pos.y.floor() as i32;
    let base_z = server_pos.z.floor() as i32;

    let trapdoor_pos = BlockPos::new(base_x + 1, base_y, base_z);
    let break_pos = BlockPos::new(base_x - 1, base_y + 1, base_z);

    world.set_block_state(
        &trapdoor_pos,
        Block::OAK_TRAPDOOR.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );
    world.set_block_state(
        &break_pos,
        Block::GOLD_BLOCK.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );

    // 1. Right-click oak trapdoor via SUseItemOn -> should toggle open
    bot1.use_block(trapdoor_pos, 1).await?;

    // 2. Left-click gold block in Creative mode via SPlayerAction(StartedDigging) -> should break to AIR
    bot1.start_digging(break_pos, 2).await?;

    let mut trapdoor_opened = false;
    let mut block_broken = false;
    for _ in 0..MAX_POLL_TICKS {
        tokio::time::sleep(Duration::from_millis(5)).await;
        server.tick();

        let state = world.get_block_state(&trapdoor_pos);
        if world.get_block(&trapdoor_pos).id == Block::OAK_TRAPDOOR.id
            && OakTrapdoorLikeProperties::from_state_id(state.id).open
        {
            trapdoor_opened = true;
        }

        if world.get_block(&break_pos).id == Block::AIR.id {
            block_broken = true;
        }

        if trapdoor_opened && block_broken {
            break;
        }
    }

    if !trapdoor_opened {
        return Err(
            "Bot SUseItemOn right-click did not open the oak trapdoor in the world".to_string(),
        );
    }
    if !block_broken {
        return Err(
            "Bot SPlayerAction(StartedDigging) in Creative mode did not break the gold block"
                .to_string(),
        );
    }

    summary_lines.push(format!(
        "[PASS] bot:world_block_use_and_break ({}ms)",
        start.elapsed().as_millis()
    ));

    run_state_hardness_test(server, bot1, &player1, break_pos, summary_lines).await
}

/// `set-block-state-hardness`: a tripwire (vanilla hardness 0) given hardness 1 survives the
/// start of a survival dig and breaks once the server-driven dig completes (30 ticks by hand).
async fn run_state_hardness_test(
    server: &Arc<Server>,
    bot1: &mut HeadlessBot,
    player: &Arc<pumpkin_core::entity::player::Player>,
    pos: BlockPos,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let world = server.get_world_from_dimension(&Dimension::OVERWORLD);
    world.set_block_state(
        &pos,
        Block::TRIPWIRE.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );
    // Every tripwire state: the wire changes state on its own (attached, powered ...).
    for wire_state in Block::TRIPWIRE.states {
        pumpkin_core::block::set_state_hardness_override(wire_state.id.as_u16(), Some(1.0));
    }
    let previous_mode = player.gamemode.load();
    player.set_gamemode(pumpkin_util::GameMode::Survival);

    bot1.start_digging(pos, 3).await?;
    let mut ticks_to_break = None;
    for tick in 0..120 {
        tokio::time::sleep(Duration::from_millis(5)).await;
        server.tick();
        if world.get_block(&pos).id != Block::TRIPWIRE.id {
            ticks_to_break = Some(tick);
            break;
        }
    }

    for wire_state in Block::TRIPWIRE.states {
        pumpkin_core::block::set_state_hardness_override(wire_state.id.as_u16(), None);
    }
    player.set_gamemode(previous_mode);
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);

    match ticks_to_break {
        Some(ticks) if ticks >= 20 => {
            summary_lines.push(format!(
                "[PASS] bot:state_hardness_override broke after {ticks} ticks ({}ms)",
                start.elapsed().as_millis()
            ));
            Ok(())
        }
        Some(ticks) => Err(format!(
            "A tripwire with hardness 1 broke after {ticks} ticks, expected about 30"
        )),
        None => Err("A tripwire with hardness 1 never broke during a survival dig".to_string()),
    }
}
