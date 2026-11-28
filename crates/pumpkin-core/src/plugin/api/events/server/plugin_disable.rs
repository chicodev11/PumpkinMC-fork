use pumpkin_macros::Event;

/// A plugin is being disabled (Paper `PluginDisableEvent`).
#[derive(Event, Clone)]
pub struct PluginDisableEvent {
    pub plugin_name: String,
}
