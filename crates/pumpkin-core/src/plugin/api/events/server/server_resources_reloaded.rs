use pumpkin_macros::Event;

/// Paper `ServerResourcesReloadedEvent`: datapacks were reloaded.
#[derive(Event, Clone)]
pub struct ServerResourcesReloadedEvent {
    pub cause: String,
}
