//! Runs `tools/redstone-compare` scenarios on Pumpkin and writes a tick-by-tick trace that
//! can be diffed against the vanilla trace from `vanilla_run.py`.
//!
//! Enabled with `REDSTONE_COMPARE=<scenarios.json>` and `TRACE_OUT=<file>`.

use std::{sync::Arc, time::Duration};

use pumpkin_core::{command::CommandSender, server::Server, world::World};
use pumpkin_data::Block;
use pumpkin_util::math::position::BlockPos;
use serde_json::Value;

fn matches(world: &World, pos: &BlockPos, candidate: &str) -> bool {
    let state = world.get_block_state(pos);
    let block = Block::from_state_id(state.id);
    let (name, props) = candidate.split_once('[').unwrap_or((candidate, ""));
    if block.name != name.strip_prefix("minecraft:").unwrap_or(name) {
        return false;
    }
    let props = props.trim_end_matches(']');
    if props.is_empty() {
        return true;
    }
    let Some(actual) = block.properties(state.id) else {
        return false;
    };
    let actual = actual.to_props();
    props.split(',').all(|kv| {
        kv.split_once('=').is_some_and(|(k, v)| {
            actual
                .iter()
                .any(|p| p.key_str() == k && p.value_str() == v)
        })
    })
}

/// One trace line: the block at each probe, each traced entity's position, then the entity
/// counts.
fn trace_row(
    world: &World,
    probes: &[BlockPos],
    candidates: &[&str],
    entities: &Value,
    counts: &Value,
    area: [i32; 6],
) -> Vec<String> {
    let mut row: Vec<String> = probes
        .iter()
        .map(|pos| {
            candidates
                .iter()
                .find(|c| matches(world, pos, c))
                .map_or_else(|| "?".to_string(), |c| c.replace("minecraft:", ""))
        })
        .collect();
    for tag in strings(entities) {
        row.push(entity_position(world, tag));
    }
    for entity_type in strings(counts) {
        row.push(entity_count(world, entity_type, area));
    }
    row
}

/// `type#n`: live entities of the type in the area (up to 10 blocks above it), like
/// `execute if entity @e[type=...,x=..,dx=..]` in `vanilla_run.py`.
fn entity_count(world: &World, entity_type: &str, area: [i32; 6]) -> String {
    let name = entity_type
        .strip_prefix("minecraft:")
        .unwrap_or(entity_type);
    let [x1, y1, z1, x2, y2, z2] = area.map(f64::from);
    let n = world
        .entities
        .load()
        .iter()
        .filter(|e| {
            let entity = e.get_entity();
            let pos = entity.pos.load();
            entity.entity_type.resource_name == name
                && !entity.is_removed()
                && (x1..x2 + 1.0).contains(&pos.x)
                && (y1..y2 + 11.0).contains(&pos.y)
                && (z1..z2 + 1.0).contains(&pos.z)
        })
        .count();
    format!("{name}#{n}")
}

/// `tag=x,y,z` of the first entity with the scoreboard tag, like `data get entity ... Pos`.
fn entity_position(world: &World, tag: &str) -> String {
    world
        .entities
        .load()
        .iter()
        .find(|e| {
            e.get_entity()
                .scoreboard_tags
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(tag)
        })
        .map_or_else(
            || format!("{tag}=none"),
            |e| {
                let pos = e.get_entity().pos.load();
                format!("{tag}={:.3},{:.3},{:.3}", pos.x, pos.y, pos.z)
            },
        )
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[expect(clippy::print_stderr, reason = "CLI tool output")]
pub fn run(server: &Arc<Server>, scenarios: &str, trace_out: &str) -> Result<(), String> {
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(scenarios).map_err(|e| format!("{scenarios}: {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    let world = server.worlds.load()[0].clone();
    let source = CommandSender::Console.into_source(server);
    let dispatcher = server.command_dispatcher.load();

    // Same coordinates as vanilla: the wire update order depends on the position.
    let area: [i32; 6] = spec["area"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .map(|v| v.as_i64().map(|v| v as i32))
                .collect::<Option<Vec<_>>>()
        })
        .and_then(|v| v.try_into().ok())
        .ok_or("scenarios.json has no area")?;
    let [x1, y1, z1, x2, _, z2] = area;
    // The bot stands in the area to keep it loaded; as a spectator it picks nothing up and
    // presses no plates, like the player-less vanilla run.
    for player in server.get_all_players() {
        let name = player.gameprofile.name.clone();
        dispatcher.handle_command(&source, &format!("gamemode spectator {name}"));
        dispatcher.handle_command(
            &source,
            &format!(
                "tp {name} {} {} {}",
                i32::midpoint(x1, x2),
                y1 + 2,
                i32::midpoint(z1, z2)
            ),
        );
    }
    let loaded = |world: &World| {
        (x1..=x2).all(|x| {
            world.is_loaded(&BlockPos::new(x, y1, z1)) && world.is_loaded(&BlockPos::new(x, y1, z2))
        })
    };
    for _ in 0..3000 {
        if loaded(&world) {
            break;
        }
        server.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    if !loaded(&world) {
        return Err("scenario area did not load".into());
    }

    let run_cmd = |command: &str| {
        if let Err(e) = dispatcher.execute_input(command, &source) {
            eprintln!("redstone-compare: `{command}` failed: {:?}", e.message);
        }
    };
    let candidates = strings(&spec["candidates"]);
    let mut out = Vec::new();
    for scenario in spec["scenarios"].as_array().ok_or("no scenarios")? {
        let name = scenario["name"].as_str().unwrap_or("?");
        for command in strings(&spec["clear"]) {
            run_cmd(command);
        }
        for _ in 0..5 {
            server.tick();
        }
        for command in strings(&scenario["setup"]) {
            run_cmd(command);
        }
        let probes: Vec<BlockPos> = scenario["probes"]
            .as_array()
            .ok_or("no probes")?
            .iter()
            .filter_map(|p| {
                let v: Vec<i32> = p
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_i64)
                    .map(|x| x as i32)
                    .collect();
                (v.len() == 3).then(|| BlockPos::new(v[0], v[1], v[2]))
            })
            .collect();
        for tick in 0..scenario["ticks"].as_u64().unwrap_or(0) {
            for event in scenario["events"].as_array().into_iter().flatten() {
                if event["tick"].as_u64() == Some(tick) {
                    for command in strings(&event["cmds"]) {
                        run_cmd(command);
                    }
                }
            }
            let row = trace_row(
                &world,
                &probes,
                &candidates,
                &scenario["entities"],
                &scenario["counts"],
                area,
            );
            out.push(format!("{name} t{tick}: {}", row.join(" | ")));
            server.tick();
        }
    }
    std::fs::write(trace_out, out.join("\n") + "\n").map_err(|e| e.to_string())?;
    eprintln!("redstone-compare: wrote {} lines to {trace_out}", out.len());
    Ok(())
}
