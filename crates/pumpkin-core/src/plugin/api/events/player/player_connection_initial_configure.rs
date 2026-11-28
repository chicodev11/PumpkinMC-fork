use pumpkin_macros::Event;

/// A connection enters the configuration phase (Paper `PlayerConnectionInitialConfigureEvent`).
#[derive(Event, Clone)]
pub struct PlayerConnectionInitialConfigureEvent {
    pub name: String,
    pub uuid: String,
}
