use std::sync::Arc;

use super::super::BrainTick;
use super::super::memory::{MemoryStatus, position_tracker::EntityTracker, types};
use super::OneShot;
use super::utils::is_holding_usable_non_melee_weapon;

/// Vanilla `MeleeAttack`: hits a visible attack target in melee reach, then cools down for
/// `cooldown` ticks.
#[must_use]
pub fn create(cooldown: i64) -> OneShot {
    create_if(|_| true, cooldown)
}

/// Vanilla `MeleeAttack.create(predicate, cooldown)`.
#[must_use]
pub fn create_if(can_attack: fn(&BrainTick<'_>) -> bool, cooldown: i64) -> OneShot {
    OneShot::new(
        "MeleeAttack",
        vec![
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (types::ATTACK_COOLING_DOWN.id(), MemoryStatus::ValueAbsent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
        ],
        move |tick| {
            let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
                return false;
            };
            if !can_attack(tick)
                || is_holding_usable_non_melee_weapon(tick.mob)
                || !tick
                    .mob
                    .get_mob_entity()
                    .is_in_attack_range(target.as_ref())
            {
                return false;
            }
            let visible = tick
                .state
                .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
                .is_some_and(|visible| visible.contains(target.as_ref(), &tick.visibility()));
            if !visible {
                return false;
            }
            tick.state.set(
                types::LOOK_TARGET,
                Arc::new(EntityTracker::new(target.clone(), true)) as _,
            );
            tick.mob.get_mob_entity().living_entity.swing_hand();
            // Vanilla plays mob attack sounds inside `doHurtTarget`; `on_attack` stands in for it.
            if tick.mob.do_hurt_target(target.as_ref()) {
                tick.mob.on_attack(target.as_ref());
            }
            tick.state
                .set_with_expiry(types::ATTACK_COOLING_DOWN, true, cooldown);
            true
        },
    )
}
