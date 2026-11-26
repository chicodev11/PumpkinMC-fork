use std::sync::Arc;

use pumpkin_data::entity::EntityType;

use crate::entity::EntityBase;

use super::super::BrainTick;
use super::super::memory::{MemoryStatus, types};
use super::one_shot::OneShot;
use super::utils::is_dead_or_dying;

#[must_use]
pub fn create(
    celebrate_duration: i32,
    wants_to_dance: impl Fn(&BrainTick<'_>, &Arc<dyn EntityBase>) -> bool + Send + Sync + 'static,
) -> OneShot {
    OneShot::with_required(
        "StartCelebratingIfTargetDead",
        vec![
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (types::CELEBRATE_LOCATION.id(), MemoryStatus::ValueAbsent),
        ],
        vec![types::ANGRY_AT.id(), types::DANCING.id()],
        move |tick| {
            let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
                return false;
            };
            if !is_dead_or_dying(target.as_ref()) {
                return false;
            }
            if wants_to_dance(tick, &target) {
                tick.state
                    .set_with_expiry(types::DANCING, true, i64::from(celebrate_duration));
            }
            tick.state.set_with_expiry(
                types::CELEBRATE_LOCATION,
                target.get_entity().block_pos.load(),
                i64::from(celebrate_duration),
            );
            if target.get_entity().entity_type != &EntityType::PLAYER
                || tick.world.level_info.load().game_rules.forgive_dead_players
            {
                tick.state.erase(types::ATTACK_TARGET.id());
                tick.state.erase(types::ANGRY_AT.id());
            }
            true
        },
    )
}
