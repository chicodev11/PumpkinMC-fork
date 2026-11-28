use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::EntityBase;
use crate::entity::player::Player;

/// Two animals breed into an egg instead of a baby (Paper `EntityFertilizeEggEvent`).
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityFertilizeEggEvent {
    pub entity: Arc<dyn EntityBase>,
    pub father: Arc<dyn EntityBase>,
    pub breeder: Option<Arc<Player>>,
    pub experience: i32,
}
