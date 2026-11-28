use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::player::Player;

/// Someone runs a command that does not exist (Paper `UnknownCommandEvent`). No message shows nothing.
#[derive(Event, Clone)]
pub struct UnknownCommandEvent {
    pub player: Option<Arc<Player>>,
    pub command_line: String,
    pub message: Option<String>,
}
