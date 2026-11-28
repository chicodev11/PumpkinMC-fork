use pumpkin_macros::Event;

/// A connection is being configured; it finishes once the handlers ran (Paper `AsyncPlayerConnectionConfigureEvent`).
#[derive(Event, Clone)]
pub struct AsyncPlayerConnectionConfigureEvent {
    pub name: String,
    pub uuid: String,
}
