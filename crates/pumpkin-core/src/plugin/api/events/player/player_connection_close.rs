use pumpkin_macros::Event;

/// Paper `PlayerConnectionCloseEvent`: a joined player's connection closed. Read-only.
#[derive(Event, Clone)]
pub struct PlayerConnectionCloseEvent {
    pub player_uuid: String,
    pub player_name: String,
}
