use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::player::Player;

/// Command completions are about to be sent (Paper `TabCompleteEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct TabCompleteEvent {
    pub player: Arc<Player>,
    pub buffer: String,
    pub completions: Vec<String>,
}
