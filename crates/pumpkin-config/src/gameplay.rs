use serde::{Deserialize, Serialize};

/// Purpur-style gameplay tweaks. Every default is vanilla behaviour; change one to opt in.
#[derive(Deserialize, Serialize, Clone, Copy)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "independent toggles")]
pub struct GameplayConfig {
    /// Creeper explosions break blocks (vanilla). `false` keeps the damage, not the holes.
    pub creeper_griefing: bool,
    /// Endermen pick up and place blocks (vanilla).
    pub enderman_griefing: bool,
    /// Phantoms spawn over players who haven't slept (vanilla).
    pub phantom_spawning: bool,
    /// Landing on farmland can turn it into dirt (vanilla).
    pub farmland_trampling: bool,
    /// A spawner mined with Silk Touch drops itself (off in vanilla).
    pub silk_touch_spawners: bool,
    /// Ticks before a dropped item despawns (vanilla 6000).
    pub item_despawn_ticks: i32,
}

impl Default for GameplayConfig {
    fn default() -> Self {
        Self {
            creeper_griefing: true,
            enderman_griefing: true,
            phantom_spawning: true,
            farmland_trampling: true,
            silk_touch_spawners: false,
            item_despawn_ticks: 6000,
        }
    }
}

static CURRENT: std::sync::RwLock<Option<GameplayConfig>> = std::sync::RwLock::new(None);

impl GameplayConfig {
    /// Makes this the configuration [`Self::current`] returns.
    pub fn install(self) {
        *CURRENT
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(self);
    }

    /// The installed configuration (the vanilla default before the server installs one).
    #[must_use]
    pub fn current() -> Self {
        CURRENT
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .unwrap_or_default()
    }
}
