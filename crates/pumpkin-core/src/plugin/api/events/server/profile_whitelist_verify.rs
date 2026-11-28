use pumpkin_macros::Event;

/// A joining profile is checked against the whitelist (Paper `ProfileWhitelistVerifyEvent`).
#[derive(Event, Clone)]
pub struct ProfileWhitelistVerifyEvent {
    pub name: String,
    pub uuid: String,
    pub whitelisted: bool,
    pub whitelist_enabled: bool,
    pub is_op: bool,
    pub kick_message: Option<String>,
}
