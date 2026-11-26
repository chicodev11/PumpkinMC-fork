use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::OneShot;

/// Vanilla `BecomePassiveIfMemoryPresent`: while `memory` is set, forget the attack target and
/// stay pacified for `pacify_duration` ticks.
#[must_use]
pub fn create(memory: MemoryModuleId, pacify_duration: i64) -> OneShot {
    OneShot::new(
        "BecomePassiveIfMemoryPresent",
        vec![
            (types::ATTACK_TARGET.id(), MemoryStatus::Registered),
            (types::PACIFIED.id(), MemoryStatus::ValueAbsent),
            (memory, MemoryStatus::ValuePresent),
        ],
        move |tick| {
            tick.state
                .set_with_expiry(types::PACIFIED, true, pacify_duration);
            tick.state.erase(types::ATTACK_TARGET.id());
            true
        },
    )
}
