use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use pumpkin_core::{command::CommandSender, server::Server};
use pumpkin_data::{Block, block_properties::StickyPistonLikeProperties, dimension::Dimension};
use pumpkin_util::math::position::BlockPos;

use crate::bot::HeadlessBot;

/// Vanilla handles packets as server tasks between ticks, before the level tick: a lever
/// pulled by a player powers a piston whose block event runs in the very next tick, like a
/// block placed by a command (the timing `tools/redstone-compare` checks against vanilla).
pub async fn run_redstone_input_timing_test(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let world = server.get_world_from_dimension(&Dimension::OVERWORLD);
    let player = server
        .get_player_by_name(&bot.name)
        .ok_or("missing bot player")?;
    let feet = player.position().to_block_pos();
    // Near the bot but inside its own (loaded) chunk; the fill below clears whatever earlier
    // bot tests left there.
    let (min_x, min_z) = (feet.0.x & !15, feet.0.z & !15);
    let (x, y, z) = (
        feet.0.x.clamp(min_x + 3, min_x + 13),
        feet.0.y,
        (feet.0.z - 3).clamp(min_z + 1, min_z + 14),
    );
    let source = CommandSender::Console.into_source(server);
    let dispatcher = server.command_dispatcher.load();
    let mut errors = Vec::new();
    for command in [
        format!(
            "fill {} {y} {} {} {} {} air",
            x - 3,
            z - 1,
            x + 2,
            y + 1,
            z + 1
        ),
        format!(
            "fill {} {} {} {} {} {} stone",
            x - 3,
            y - 1,
            z - 1,
            x + 2,
            y - 1,
            z + 1
        ),
        format!("setblock {} {y} {z} lever[face=floor,facing=east]", x - 2),
        format!("setblock {} {y} {z} piston[facing=east]", x - 1),
        format!("setblock {x} {y} {z} stone"),
    ] {
        // A fill over blocks that are already right fails like vanilla; that is fine here.
        if let Err(e) = dispatcher.execute_input(&command, &source) {
            errors.push(format!("`{command}`: {:?}", e.message));
        }
    }
    if world.get_block(&BlockPos::new(x - 1, y, z)) != &Block::PISTON {
        return Err(format!("redstone input timing: setup failed: {errors:?}"));
    }
    for _ in 0..3 {
        server.tick();
    }

    let piston = BlockPos::new(x - 1, y, z);
    bot.set_held_item(4).await?;
    bot.use_block(BlockPos::new(x - 2, y, z), 40).await?;
    // Let the packet reach the inbound queue, then run exactly one tick.
    tokio::time::sleep(Duration::from_millis(200)).await;
    server.tick();

    let state = world.get_block_state(&piston);
    let extended = world.get_block(&piston) == &Block::PISTON
        && StickyPistonLikeProperties::from_state_id(state.id).extended;
    let moving = world.get_block(&BlockPos::new(x, y, z)) == &Block::MOVING_PISTON;
    if !(extended && moving) {
        return Err(format!(
            "Piston did not start extending in the tick after the lever packet \
             (piston {}, front {})",
            world.get_block(&piston).name,
            world.get_block(&BlockPos::new(x, y, z)).name
        ));
    }

    summary_lines.push(format!(
        "- Redstone input timing: lever packet moves the piston in the next tick ({:?})",
        start.elapsed()
    ));
    Ok(())
}
