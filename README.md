<div align="center">

# Pumpkin (fork)

</div>

> [!IMPORTANT]
> **This fork is written with AI.** Every change on top of upstream Pumpkin (since 2026-11-08) was
> written by an AI coding agent (Claude, by Anthropic) under the maintainer's direction. Commits
> are made by the maintainer with Claude as co-author.
>
> It is a personal fork, not affiliated with or endorsed by the Pumpkin project. Please don't
> report this fork's bugs to Pumpkin-MC/Pumpkin.

A Minecraft server written in Rust. This is a personal fork of Pumpkin that aims to behave like
the vanilla Java server as closely as possible (99%+, Paper-style: vanilla quirks such as TNT
duplication stay), with a plugin API on top. It is meant for running a small survival server with
friends.

It runs offline from any outside service: no telemetry, no update checks, no marketplace or
other network calls home.

## Running

Build a release binary (needs the Rust toolchain from `rust-toolchain.toml`; the first build
downloads Minecraft data from Mojang):

```sh
cargo build --release --bin pumpkin
```

Put `target/release/pumpkin` (`pumpkin.exe` on Windows) in an empty server folder and run it
there with the restart script, which brings the server back after a crash:

```sh
scripts/run-server.sh ./pumpkin          # Linux / macOS
scripts\run-server.bat pumpkin.exe       # Windows
```

The first start writes `pumpkin.toml` with every setting. The defaults are 1000 players, view
distance 16 and simulation distance 10. A `pumpkin.toml` written earlier keeps its own values (new
settings are added to it), so change them there.

Clients join with Minecraft Java 26.3 (Bedrock clients work too, with gaps).

## Safety nets

The server does what vanilla does:

- **Hangs and crashes**: a watchdog stops a server whose tick has been stuck for 60 seconds
  (vanilla `max-tick-time`) and writes a report to `crash-reports/`; panics save the world and
  write a crash report. `run-server.sh` restarts the server, and gives up after 5 crashes in 10
  minutes.
- **Unreadable chunks**: as in vanilla, the error is logged, a chunk IO error report is written
  to `debug/`, and the chunk is generated again.
- **Safe saving**: player data, maps and `level.dat` are written to a temporary file and swapped
  in, and player data keeps the previous save as `<uuid>.dat_old`, which is loaded if the current
  one cannot be read.

World backups, `/bugreport` and inventory snapshots with `/restoreinv` are not in vanilla, so
they live in a separate `safety` plugin that is not part of this repository.

## Development

`cargo run -p pumpkin-tester` runs the bot tests and GameTests, and
`tools/redstone-compare` compares redstone timing against a vanilla server.

## License & Attribution

* **Pumpkin Server**: based on Pumpkin (Pumpkin-MC/Pumpkin, commit `d0c0a5d73`; this repository
  starts from a single initial commit), licensed under the
  [GNU General Public License v3.0 (GPLv3)](LICENSE).
* **Plugin API (`pumpkin-plugin-api` & `pumpkin-plugin-wit`)**: Dual-licensed under
  [MIT](crates/pumpkin-plugin-api/LICENSE-MIT) OR [Apache-2.0](crates/pumpkin-plugin-api/LICENSE-APACHE).
* **Third-Party Assets & Data**: Bedrock mappings, protocol conversion data, and Minecraft assets
  are subject to their respective licenses and attribution terms. See
  [assets/NOTICE.md](assets/NOTICE.md) for full details.
