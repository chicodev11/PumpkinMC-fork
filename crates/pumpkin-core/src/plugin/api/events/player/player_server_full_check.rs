use pumpkin_macros::Event;

/// A joining player is checked against the player limit (Paper `PlayerServerFullCheckEvent`).
#[derive(Event, Clone)]
pub struct PlayerServerFullCheckEvent {
    pub name: String,
    pub uuid: String,
    pub allowed: bool,
}
