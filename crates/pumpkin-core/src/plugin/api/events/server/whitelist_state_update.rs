use pumpkin_macros::{Event, cancellable};

/// A profile is added to or removed from the whitelist (Paper `WhitelistStateUpdateEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct WhitelistStateUpdateEvent {
    pub name: String,
    pub uuid: String,
    pub status: String,
}
