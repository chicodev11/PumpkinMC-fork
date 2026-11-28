use pumpkin_macros::Event;

/// The whitelist is turned on or off (Paper `WhitelistToggleEvent`).
#[derive(Event, Clone)]
pub struct WhitelistToggleEvent {
    pub enabled: bool,
}
