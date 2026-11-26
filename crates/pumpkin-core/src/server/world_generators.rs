//! Named plugin chunk generators for the server's default worlds.
//!
//! Like Paper's `bukkit.yml` `worlds.<world>.generator: <Plugin>:<id>`: the advanced config's
//! `[world.generators]` maps a dimension to `"<plugin name>:<generator name>"`, and the plugin
//! registers that name.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use pumpkin_data::dimension::Dimension;

use super::WorldGeneratorChoice;

/// Builds the generator for a world of `Dimension` with the world seed; `None` once the plugin
/// that registered it is gone.
pub type GeneratorFactory =
    Arc<dyn Fn(Dimension, u64) -> Option<WorldGeneratorChoice> + Send + Sync>;

static GENERATORS: LazyLock<RwLock<HashMap<String, GeneratorFactory>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn key(plugin: &str, name: &str) -> String {
    format!("{}:{}", plugin.to_lowercase(), name.to_lowercase())
}

/// Registers `plugin`'s generator `name`, replacing an earlier one of the same name.
pub fn register(plugin: &str, name: &str, factory: GeneratorFactory) {
    GENERATORS
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key(plugin, name), factory);
}

/// The generator a config entry `"<plugin>:<name>"` names (case-insensitive).
#[must_use]
pub fn get(spec: &str) -> Option<GeneratorFactory> {
    let (plugin, name) = spec.split_once(':')?;
    GENERATORS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key(plugin.trim(), name.trim()))
        .cloned()
}
