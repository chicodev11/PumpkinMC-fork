//! Client-side block state remapping.
//!
//! Plugins that add custom blocks (CraftEngine-style) keep a vanilla "carrier" state on the
//! server (for example a note block state) and show players another state, which a resource
//! pack then retextures. This table holds that "server state -> client state" mapping. Every
//! clientbound Java packet that carries a block state reads it while it is written, so the
//! server world, physics and plugins keep seeing the real state.
//!
//! The table is process-wide and lock free. While it maps nothing, [`client_state`] is a compare
//! and a single atomic load. Changing it does not resend chunks players already have.
//!
//! Custom (registered) states are always shown as the vanilla state they were registered with
//! (see [`crate::custom_block`]); this table only covers vanilla states.

use std::sync::{
    LazyLock,
    atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering},
};

use crate::{BlockState, BlockStateId};

static TABLE: LazyLock<Box<[AtomicU16]>> =
    LazyLock::new(|| (0..BlockStateId::COUNT).map(AtomicU16::new).collect());

/// How many states currently map to something other than themselves.
static REMAPPED: AtomicUsize = AtomicUsize::new(0);

/// Bumped on every change, so caches of encoded packets know they are stale.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Changes whenever the mapping changes.
#[inline]
#[must_use]
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// Whether any state is shown as another: a plugin remap, or any custom state existing (custom
/// states are always shown as their vanilla client state). Packet writers skip the per-state
/// lookup while this is false.
#[inline]
#[must_use]
pub fn is_active() -> bool {
    REMAPPED.load(Ordering::Relaxed) != 0 || crate::custom_block::any_registered()
}

/// The state id a client is shown for the server state `state`.
#[inline]
#[must_use]
pub fn client_state(state: u16) -> u16 {
    if state >= BlockStateId::COUNT {
        return custom_client_state(state);
    }
    if !is_active() {
        return state;
    }
    TABLE
        .get(usize::from(state))
        .map_or(state, |entry| entry.load(Ordering::Relaxed))
}

#[cold]
fn custom_client_state(state: u16) -> u16 {
    BlockStateId::new(state).map_or(state, |id| crate::custom_block::client_state(id).as_u16())
}

/// [`client_state`] for the `i32` ids packets store; ids outside the registry pass through.
#[inline]
#[must_use]
pub fn client_state_i32(state: i32) -> i32 {
    u16::try_from(state).map_or(state, |state| i32::from(client_state(state)))
}

/// Shows `client` to players wherever the server has `server`. Returns the previous mapping.
/// Custom states are not in this table (they always show their registered client state), so
/// for them, and for a custom `client`, this does nothing and returns `server`.
pub fn set(server: BlockStateId, client: BlockStateId) -> BlockStateId {
    if server.is_custom() || client.is_custom() {
        return server;
    }
    let previous = TABLE[usize::from(server.as_u16())].swap(client.as_u16(), Ordering::Relaxed);
    let was_remapped = previous != server.as_u16();
    let is_remapped = client != server;
    match (was_remapped, is_remapped) {
        (false, true) => {
            REMAPPED.fetch_add(1, Ordering::Relaxed);
        }
        (true, false) => {
            REMAPPED.fetch_sub(1, Ordering::Relaxed);
        }
        _ => {}
    }
    if previous != client.as_u16() {
        GENERATION.fetch_add(1, Ordering::Release);
    }
    BlockStateId::new(previous).unwrap_or(server)
}

/// Shows `server` as itself again.
pub fn reset(server: BlockStateId) {
    set(server, server);
}

/// Removes every mapping.
pub fn reset_all() {
    for (id, entry) in TABLE.iter().enumerate() {
        entry.store(id as u16, Ordering::Relaxed);
    }
    REMAPPED.store(0, Ordering::Relaxed);
    GENERATION.fetch_add(1, Ordering::Release);
}

/// How many server states are shown as another state.
#[must_use]
pub fn remapped_count() -> usize {
    REMAPPED.load(Ordering::Relaxed)
}

/// The Bedrock runtime id a client is shown for the server state `state`.
#[inline]
#[must_use]
pub fn bedrock_runtime_id(state: BlockStateId) -> u32 {
    let shown = BlockStateId::new(client_state(state.as_u16())).unwrap_or(state);
    BlockState::to_be_network_id(shown)
}

/// Serializes the tests that touch the process-wide table (also from other crates).
#[doc(hidden)]
pub static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    /// Custom states always show their registered vanilla client state.
    #[test]
    fn custom_states_show_their_client_state() {
        let _guard = TEST_LOCK.lock().unwrap();
        let _registry = crate::custom_block::TEST_LOCK.lock().unwrap();
        reset_all();
        let block = crate::custom_block::test_block().block;
        let lit = block.states[0].id;
        let client = crate::Block::NOTE_BLOCK.states[1].id;
        assert!(is_active());
        assert_eq!(client_state(lit.as_u16()), client.as_u16());
        assert_eq!(
            client_state_i32(i32::from(lit.as_u16())),
            i32::from(client.as_u16())
        );
        assert_eq!(
            bedrock_runtime_id(lit),
            BlockState::to_be_network_id(client)
        );
        // The vanilla table cannot remap custom states.
        assert_eq!(set(lit, crate::Block::STONE.default_state.id), lit);
        assert_eq!(remapped_count(), 0);
    }

    #[test]
    fn mapping_counts_and_resets() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_all();
        let stone = BlockStateId::new(1).unwrap();
        let dirt = BlockStateId::new(10).unwrap();
        assert_eq!(remapped_count(), 0);
        assert_eq!(client_state(1), 1);

        assert_eq!(set(stone, dirt), stone);
        assert_eq!(remapped_count(), 1);
        assert_eq!(client_state(1), 10);
        assert_eq!(client_state_i32(1), 10);
        assert_eq!(client_state_i32(-1), -1);
        assert_eq!(client_state(u16::MAX), u16::MAX);

        // Remapping again does not count twice; mapping back to itself clears it.
        set(stone, dirt);
        assert_eq!(remapped_count(), 1);
        reset(stone);
        assert_eq!(remapped_count(), 0);
        assert_eq!(client_state(1), 1);
    }

    /// Bedrock runtime ids follow the mapping.
    #[test]
    fn bedrock_runtime_ids_follow_the_mapping() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_all();
        let stone = BlockStateId::new(1).unwrap();
        let dirt = BlockStateId::new(10).unwrap();
        set(stone, dirt);
        assert_eq!(
            bedrock_runtime_id(stone),
            BlockState::to_be_network_id(dirt)
        );
        reset_all();
        assert_eq!(
            bedrock_runtime_id(stone),
            BlockState::to_be_network_id(stone)
        );
    }
}
