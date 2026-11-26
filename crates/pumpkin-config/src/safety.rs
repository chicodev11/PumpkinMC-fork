//! The hang watchdog (vanilla `max-tick-time`).

use serde::{Deserialize, Serialize};

/// Hang detection. World backups, bug reports and inventory snapshots are in the official
/// `safety` plugin (kept outside this repository), since vanilla has none of them.
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct SafetyConfig {
    /// Detects a stuck main tick loop.
    pub watchdog: WatchdogConfig,
}

/// Vanilla `ServerWatchdog`: stop a server whose tick loop stopped making progress, so a
/// restart script can bring it back instead of players sitting in a frozen world.
#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct WatchdogConfig {
    /// Whether the watchdog runs.
    pub enabled: bool,
    /// Seconds a single tick may take before the server is considered hung.
    pub max_tick_seconds: u64,
    /// Log a warning for each tick slower than this many milliseconds; 0 (the default, as
    /// vanilla only logs "Can't keep up!") turns it off.
    pub slow_tick_warning_ms: u64,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_tick_seconds: 60,
            slow_tick_warning_ms: 0,
        }
    }
}
