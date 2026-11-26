//! Block states whose vanilla behaviour is switched off.
//!
//! Custom block plugins put their blocks on vanilla "carrier" states (note block, tripwire,
//! mushroom stem, ...) and show them to players through
//! [`pumpkin_protocol::block_remap`]. Vanilla would keep changing those states: a note block
//! retunes when used, picks a new instrument when the block below changes and plays on
//! redstone. A frozen state keeps all of that from happening, so the plugin alone decides what
//! the block does:
//!
//! - shape updates from neighbours keep the state as it is,
//! - neighbour (redstone) updates, scheduled ticks and random ticks are skipped,
//! - using it does nothing, so the held item is used instead (placing a block against it),
//! - attacking it does nothing.
//!
//! Breaking, collision, drops and plugin events still work as for the carrier block. The set
//! is process-wide and lock free; while it is empty every check is one atomic load.

use std::sync::{
    LazyLock,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use pumpkin_data::BlockStateId;

static BITS: LazyLock<Box<[AtomicU64]>> = LazyLock::new(|| {
    // Covers the whole u16 id space, custom states included.
    (0..(usize::from(u16::MAX) + 1).div_ceil(64))
        .map(|_| AtomicU64::new(0))
        .collect()
});

static FROZEN: AtomicUsize = AtomicUsize::new(0);

fn slot(state: BlockStateId) -> (usize, u64) {
    let id = usize::from(state.as_u16());
    (id / 64, 1u64 << (id % 64))
}

/// Whether any state is frozen.
#[inline]
#[must_use]
pub fn is_active() -> bool {
    FROZEN.load(Ordering::Relaxed) != 0
}

/// Whether `state` has its vanilla behaviour switched off.
#[inline]
#[must_use]
pub fn is_frozen(state: BlockStateId) -> bool {
    if !is_active() {
        return false;
    }
    let (word, bit) = slot(state);
    BITS[word].load(Ordering::Relaxed) & bit != 0
}

/// [`is_frozen`] for the state at `position`; reads the world only while something is frozen.
#[inline]
#[must_use]
pub fn is_frozen_at(
    world: &crate::world::World,
    position: &pumpkin_util::math::position::BlockPos,
) -> bool {
    is_active() && is_frozen(world.get_block_state_id(position))
}

/// Switches off the vanilla behaviour of `state`. Returns whether it was not frozen yet.
pub fn freeze(state: BlockStateId) -> bool {
    let (word, bit) = slot(state);
    let newly = BITS[word].fetch_or(bit, Ordering::Relaxed) & bit == 0;
    if newly {
        FROZEN.fetch_add(1, Ordering::Relaxed);
    }
    newly
}

/// Gives `state` its vanilla behaviour back. Returns whether it was frozen.
pub fn unfreeze(state: BlockStateId) -> bool {
    let (word, bit) = slot(state);
    let was = BITS[word].fetch_and(!bit, Ordering::Relaxed) & bit != 0;
    if was {
        FROZEN.fetch_sub(1, Ordering::Relaxed);
    }
    was
}

/// Gives every state its vanilla behaviour back.
pub fn unfreeze_all() {
    for word in BITS.iter() {
        word.store(0, Ordering::Relaxed);
    }
    FROZEN.store(0, Ordering::Relaxed);
}

/// How many states are frozen.
#[must_use]
pub fn frozen_count() -> usize {
    FROZEN.load(Ordering::Relaxed)
}

/// Serializes tests that touch the process-wide set.
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freeze_and_unfreeze_count_once() {
        let _guard = TEST_LOCK.lock().unwrap();
        unfreeze_all();
        let last = BlockStateId::new(BlockStateId::COUNT - 1).unwrap();
        assert!(!is_frozen(last));
        assert!(freeze(last));
        assert!(!freeze(last));
        assert!(is_frozen(last));
        assert_eq!(frozen_count(), 1);
        assert!(unfreeze(last));
        assert!(!unfreeze(last));
        assert!(!is_active());
    }
}
