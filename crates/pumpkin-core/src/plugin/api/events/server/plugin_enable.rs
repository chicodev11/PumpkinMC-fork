use pumpkin_macros::Event;

/// A plugin was enabled (Paper `PluginEnableEvent`).
#[derive(Event, Clone)]
pub struct PluginEnableEvent {
    pub plugin_name: String,
}
