//! The watchdog for a hung tick loop, like vanilla's `ServerWatchdog`.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pumpkin_config::safety::WatchdogConfig;
use tracing::{error, warn};

use super::Server;

/// What the tick loop is doing, for the watchdog's report.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TickPhase {
    Idle = 0,
    TickStartEvent = 1,
    Worlds = 2,
    GameTests = 3,
    TickEndEvent = 4,
}

impl TickPhase {
    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::TickStartEvent,
            2 => Self::Worlds,
            3 => Self::GameTests,
            4 => Self::TickEndEvent,
            _ => Self::Idle,
        }
    }

    const fn describe(self) -> &'static str {
        match self {
            Self::Idle => "waiting for the next tick",
            Self::TickStartEvent => "running plugin tick start handlers",
            Self::Worlds => "ticking the worlds",
            Self::GameTests => "running game tests",
            Self::TickEndEvent => "running plugin tick end handlers",
        }
    }
}

static TICK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static TICK_STARTED_MILLIS: AtomicU64 = AtomicU64::new(0);
static TICK_PHASE: AtomicU8 = AtomicU8::new(TickPhase::Idle as u8);
/// One bit per world (by index) still inside its tick.
static WORLDS_TICKING: AtomicU64 = AtomicU64::new(0);

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Marks the start of a server tick for the watchdog.
pub fn begin_tick() {
    TICK_STARTED_MILLIS.store(now_millis(), Ordering::Relaxed);
    TICK_IN_PROGRESS.store(true, Ordering::Release);
}

/// Marks the end of a server tick.
pub fn end_tick() {
    TICK_IN_PROGRESS.store(false, Ordering::Release);
    set_phase(TickPhase::Idle);
}

pub fn set_phase(phase: TickPhase) {
    TICK_PHASE.store(phase as u8, Ordering::Relaxed);
}

/// Marks world `index` as ticking (`true`) or done (`false`).
pub fn mark_world_ticking(index: usize, ticking: bool) {
    let Some(bit) = 1u64.checked_shl(index as u32) else {
        return;
    };
    if ticking {
        WORLDS_TICKING.fetch_or(bit, Ordering::Relaxed);
    } else {
        WORLDS_TICKING.fetch_and(!bit, Ordering::Relaxed);
    }
}

/// Starts the watchdog thread.
pub fn start(server: &Arc<Server>) {
    let safety = &server.advanced_config.safety;
    if safety.watchdog.enabled && safety.watchdog.max_tick_seconds > 0 {
        let server = server.clone();
        if let Err(err) = std::thread::Builder::new()
            .name("Server-Watchdog".into())
            .spawn(move || watchdog_loop(&server))
        {
            error!("Failed to start the watchdog: {err}");
        }
    }
}

/// Logs a tick slower than the configured threshold.
pub fn report_slow_tick(config: &WatchdogConfig, tick_duration_nanos: i64) {
    if config.slow_tick_warning_ms == 0 {
        return;
    }
    let millis = u64::try_from(tick_duration_nanos / 1_000_000).unwrap_or(0);
    if millis >= config.slow_tick_warning_ms {
        warn!(
            "Slow tick: {millis}ms (limit {}ms)",
            config.slow_tick_warning_ms
        );
    }
}

fn watchdog_loop(server: &Arc<Server>) {
    let config = &server.advanced_config.safety.watchdog;
    let limit_millis = config.max_tick_seconds.saturating_mul(1000);
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if server.stop_token.is_cancelled() {
            return;
        }
        if !TICK_IN_PROGRESS.load(Ordering::Acquire) {
            continue;
        }
        let elapsed = now_millis().saturating_sub(TICK_STARTED_MILLIS.load(Ordering::Relaxed));
        if elapsed < limit_millis {
            continue;
        }
        let report = hang_report(server, elapsed);
        error!("{report}");
        write_report(&report);
        error!("The server is not responding and will stop so it can be restarted.");
        // A hung tick holds locks the normal shutdown needs, so no save is attempted:
        // the last autosave is what survives.
        std::process::exit(2);
    }
}

fn hang_report(server: &Arc<Server>, elapsed_millis: u64) -> String {
    use std::fmt::Write;
    let phase = TickPhase::from_u8(TICK_PHASE.load(Ordering::Relaxed));
    let mut report = String::new();
    let _ = writeln!(report, "---- Pumpkin watchdog report ----");
    let _ = writeln!(
        report,
        "A single server tick took {:.1}s (limit {}s).",
        elapsed_millis as f64 / 1000.0,
        server.advanced_config.safety.watchdog.max_tick_seconds
    );
    let _ = writeln!(
        report,
        "Tick #{}, stuck while {}.",
        server.tick_count.load(Ordering::Relaxed),
        phase.describe()
    );
    if phase == TickPhase::Worlds {
        let ticking = WORLDS_TICKING.load(Ordering::Relaxed);
        for (index, world) in server.worlds.load().iter().enumerate() {
            if index < 64 && ticking & (1 << index) != 0 {
                let _ = writeln!(
                    report,
                    "World still ticking: {}",
                    world.dimension.minecraft_name
                );
            }
        }
    }
    let _ = writeln!(
        report,
        "Players online: {}",
        server
            .get_all_players()
            .iter()
            .map(|p| p.gameprofile.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );
    report
}

fn write_report(report: &str) {
    let dir = Path::new("crash-reports");
    let path = dir.join(format!("watchdog-{}.txt", timestamp()));
    if let Err(err) = fs::create_dir_all(dir).and_then(|()| fs::write(&path, report)) {
        error!("Failed to write the watchdog report: {err}");
    } else {
        error!("Watchdog report written to {}", path.display());
    }
}

pub(crate) fn timestamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}
