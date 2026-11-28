use pumpkin_macros::{Event, cancellable};

/// A command arrives over RCON (Paper `RemoteServerCommandEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct RemoteServerCommandEvent {
    pub command: String,
}
