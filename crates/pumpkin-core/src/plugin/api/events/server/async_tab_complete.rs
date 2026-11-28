use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::player::Player;

/// A player asks for command completions (Paper `AsyncTabCompleteEvent`). A handled event replaces the server's completions.
#[cancellable]
#[derive(Event, Clone)]
pub struct AsyncTabCompleteEvent {
    pub player: Arc<Player>,
    pub buffer: String,
    pub completions: Vec<String>,
    pub is_command: bool,
    pub handled: bool,
}
