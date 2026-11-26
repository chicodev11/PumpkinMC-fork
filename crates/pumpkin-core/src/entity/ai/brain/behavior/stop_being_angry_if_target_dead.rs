use pumpkin_data::entity::EntityType;

use super::super::memory::{MemoryStatus, types};
use super::one_shot::OneShot;
use super::utils::{get_living_entity_from_uuid_memory, is_dead_or_dying};

#[must_use]
pub fn create() -> OneShot {
    OneShot::new(
        "StopBeingAngryIfTargetDead",
        vec![(types::ANGRY_AT.id(), MemoryStatus::ValuePresent)],
        |tick| {
            let Some(target) =
                get_living_entity_from_uuid_memory(tick.state, tick.world, types::ANGRY_AT)
            else {
                return true;
            };
            if !is_dead_or_dying(target.as_ref()) {
                return true;
            }
            let forgiven = target.get_entity().entity_type != &EntityType::PLAYER
                || tick.world.level_info.load().game_rules.forgive_dead_players;
            if forgiven {
                tick.state.erase(types::ANGRY_AT.id());
            }
            true
        },
    )
}
