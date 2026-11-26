//! Vanilla `server.properties` support.
//!
//! When the file exists next to the server, its keys are applied on top of `pumpkin.toml`, so
//! panels and tools that edit `server.properties` work.

use std::net::IpAddr;
use std::num::NonZero;
use std::str::FromStr;

use pumpkin_util::world_seed::Seed;
use pumpkin_util::{Difficulty, GameMode, PermissionLvl};

use crate::PumpkinConfig;

/// Parses Java `.properties` text into key/value pairs.
///
/// Handles `#`/`!` comments, `=` or `:` separators and backslash escapes.
#[must_use]
pub fn parse_properties(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut logical = String::new();
    for raw in text.lines() {
        let line = raw.trim_start();
        if logical.is_empty() && (line.is_empty() || line.starts_with('#') || line.starts_with('!'))
        {
            continue;
        }
        // An odd number of trailing backslashes continues the line.
        let trailing = line.chars().rev().take_while(|c| *c == '\\').count();
        if trailing % 2 == 1 {
            logical.push_str(&line[..line.len() - 1]);
            continue;
        }
        logical.push_str(line);
        let entry = std::mem::take(&mut logical);
        let mut key = String::new();
        let mut chars = entry.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(next) = chars.next() {
                        key.push(unescape(next, &mut chars));
                    }
                }
                '=' | ':' => break,
                c if c.is_whitespace() => {
                    while chars.peek().is_some_and(|c| c.is_whitespace()) {
                        chars.next();
                    }
                    if chars.peek().is_some_and(|c| *c == '=' || *c == ':') {
                        chars.next();
                    }
                    break;
                }
                c => key.push(c),
            }
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut value = String::new();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    value.push(unescape(next, &mut chars));
                }
            } else {
                value.push(c);
            }
        }
        pairs.push((key, value));
    }
    pairs
}

fn unescape(c: char, rest: &mut std::iter::Peekable<std::str::Chars<'_>>) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        'f' => '\u{c}',
        'u' => {
            let hex: String = (0..4).filter_map(|_| rest.next()).collect();
            u32::from_str_radix(&hex, 16)
                .ok()
                .and_then(char::from_u32)
                .unwrap_or('\u{fffd}')
        }
        other => other,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn distance(value: &str) -> Option<NonZero<u8>> {
    let n: u8 = value.trim().parse().ok()?;
    NonZero::new(n.clamp(2, 32))
}

impl PumpkinConfig {
    /// Applies vanilla `server.properties` keys.
    ///
    /// Returns the keys that were present but could not be used (unknown or a bad value).
    pub fn apply_server_properties(&mut self, text: &str) -> Vec<String> {
        let mut skipped = Vec::new();
        for (key, value) in parse_properties(text) {
            if !self.apply_property(&key, &value) {
                skipped.push(key);
            }
        }
        skipped
    }

    #[allow(clippy::too_many_lines)]
    fn apply_property(&mut self, key: &str, value: &str) -> bool {
        let basic = &mut self.basic;
        let net = &mut self.advanced.networking;
        let applied = match key {
            "server-port" => value.trim().parse::<u16>().ok().map(|port| {
                net.java.address.set_port(port);
            }),
            "server-ip" => {
                if value.trim().is_empty() {
                    Some(())
                } else {
                    IpAddr::from_str(value.trim()).ok().map(|ip| {
                        net.java.address.set_ip(ip);
                    })
                }
            }
            "motd" => {
                net.java.motd = value.to_string();
                Some(())
            }
            "max-players" => value.trim().parse().ok().map(|n| net.java.max_players = n),
            "online-mode" => parse_bool(value).map(|b| {
                net.java.online_mode = b;
                if b {
                    net.java.encryption = true;
                }
            }),
            "view-distance" => distance(value).map(|d| net.java.view_distance = d),
            "simulation-distance" => distance(value).map(|d| net.java.simulation_distance = d),
            "difficulty" => {
                let v = value.trim();
                Difficulty::from_str(v)
                    .ok()
                    .or(match v {
                        "0" => Some(Difficulty::Peaceful),
                        "1" => Some(Difficulty::Easy),
                        "2" => Some(Difficulty::Normal),
                        "3" => Some(Difficulty::Hard),
                        _ => None,
                    })
                    .map(|d| basic.default_difficulty = d)
            }
            "gamemode" => GameMode::from_str(value.trim())
                .ok()
                .map(|g| basic.default_gamemode = g),
            "force-gamemode" => parse_bool(value).map(|b| basic.force_gamemode = b),
            "hardcore" => parse_bool(value).map(|b| basic.hardcore = b),
            "level-name" => (!value.trim().is_empty()).then(|| {
                basic.default_level_name = value.trim().to_string();
            }),
            "level-seed" => {
                if !value.trim().is_empty() {
                    basic.seed = Seed::from(value);
                }
                Some(())
            }
            "allow-nether" => parse_bool(value).map(|b| basic.allow_nether = b),
            "white-list" => parse_bool(value).map(|b| basic.white_list = b),
            "enforce-whitelist" => parse_bool(value).map(|b| basic.enforce_whitelist = b),
            "accepts-transfers" => parse_bool(value).map(|b| basic.accepts_transfers = b),
            "spawn-protection" => value
                .trim()
                .parse()
                .ok()
                .map(|n| basic.spawn_protection = n),
            "op-permission-level" => match value.trim() {
                "0" => Some(PermissionLvl::Zero),
                "1" => Some(PermissionLvl::One),
                "2" => Some(PermissionLvl::Two),
                "3" => Some(PermissionLvl::Three),
                "4" => Some(PermissionLvl::Four),
                _ => None,
            }
            .map(|lvl| basic.op_permission_level = lvl),
            "enforce-secure-profile" => parse_bool(value).map(|b| {
                // Chat reports need online mode in Pumpkin.
                basic.allow_chat_reports = b && net.java.online_mode;
            }),
            "pvp" => parse_bool(value).map(|b| self.advanced.pvp.enabled = b),
            "enable-query" => parse_bool(value).map(|b| net.query.enabled = b),
            "query.port" => value.trim().parse::<u16>().ok().map(|port| {
                net.query.address.set_port(port);
            }),
            "enable-rcon" => parse_bool(value).map(|b| net.rcon.enabled = b),
            "rcon.port" => value.trim().parse::<u16>().ok().map(|port| {
                net.rcon.address.set_port(port);
            }),
            "rcon.password" => {
                net.rcon.password = value.to_string();
                Some(())
            }
            _ => None,
        };
        applied.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vanilla_properties() {
        let text = "#Minecraft server properties\n#Sun Nov 22 2026\nmotd=A \\u00A7aGreen\\: server\nserver-port=25570\nlevel-seed=\nmax-players = 7\nunknown-key=1\n";
        let pairs = parse_properties(text);
        assert_eq!(pairs[0], ("motd".into(), "A \u{a7}aGreen: server".into()));
        assert_eq!(pairs[3], ("max-players".into(), "7".into()));

        let mut config = PumpkinConfig::default();
        let skipped = config.apply_server_properties(text);
        assert_eq!(skipped, vec!["unknown-key".to_string()]);
        assert_eq!(config.advanced.networking.java.address.port(), 25570);
        assert_eq!(config.advanced.networking.java.max_players, 7);
        assert_eq!(
            config.advanced.networking.java.motd,
            "A \u{a7}aGreen: server"
        );
    }

    #[test]
    fn rcon_and_gameplay_keys() {
        let mut config = PumpkinConfig::default();
        config.apply_server_properties(
            "enable-rcon=true\nrcon.port=25580\nrcon.password=secret\ndifficulty=hard\ngamemode=creative\npvp=false\n",
        );
        assert!(config.advanced.networking.rcon.enabled);
        assert_eq!(config.advanced.networking.rcon.address.port(), 25580);
        assert_eq!(config.advanced.networking.rcon.password, "secret");
        assert_eq!(config.basic.default_difficulty, Difficulty::Hard);
        assert_eq!(config.basic.default_gamemode, GameMode::Creative);
        assert!(!config.advanced.pvp.enabled);
    }
}
