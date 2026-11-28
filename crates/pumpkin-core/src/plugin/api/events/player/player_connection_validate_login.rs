use pumpkin_macros::Event;

/// A connection is checked before it may join (Paper `PlayerConnectionValidateLoginEvent`). A kick message refuses it.
#[derive(Event, Clone)]
pub struct PlayerConnectionValidateLoginEvent {
    pub name: String,
    pub uuid: String,
    pub kick_message: Option<String>,
}
