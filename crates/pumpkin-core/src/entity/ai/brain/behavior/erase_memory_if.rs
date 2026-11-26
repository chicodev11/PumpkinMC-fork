use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, MemoryStatus};
use super::OneShot;

/// Vanilla `EraseMemoryIf`: erases `memory` whenever `predicate` holds.
#[must_use]
pub fn create(predicate: fn(&BrainTick<'_>) -> bool, memory: MemoryModuleId) -> OneShot {
    OneShot::new(
        "EraseMemoryIf",
        vec![(memory, MemoryStatus::ValuePresent)],
        move |tick| {
            if predicate(tick) {
                tick.state.erase(memory);
                true
            } else {
                false
            }
        },
    )
}
