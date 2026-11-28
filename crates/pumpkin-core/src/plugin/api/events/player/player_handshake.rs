use pumpkin_macros::{Event, cancellable};

/// A connection sends its handshake (Paper `PlayerHandshakeEvent`). Leaving it cancelled keeps the default handling.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerHandshakeEvent {
    pub original_handshake: String,
    pub server_hostname: String,
    pub failed: bool,
    pub fail_message: String,
}
