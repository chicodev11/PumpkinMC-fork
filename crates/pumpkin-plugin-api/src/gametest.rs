//! Gametest callbacks.
//!
//! The `gametest` host API takes callback ids where a plugin passes a closure. Register the
//! closure here to get an id, then pass the id to the host function:
//!
//! ```rust,ignore
//! use pumpkin_plugin_api::gametest::{self, gametest_wit};
//!
//! let test = gametest::test_callback(|test| {
//!     let _ = test.succeed();
//! });
//! gametest_wit::register("example", "succeeds", test)?;
//! ```
//!
//! Callbacks stay registered for the plugin's lifetime; [`remove`] drops one that is no longer
//! needed.

use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};

use crate::BoxFuture;
pub use crate::wit::pumpkin::plugin::gametest as gametest_wit;
use crate::wit::pumpkin::plugin::gametest::{
    AsyncTestCallbackId, BlockPermutation, BlockPredicateCallbackId, EntityPredicateCallbackId,
    Test, TestCallbackId, VoidCallbackId,
};
use crate::wit::pumpkin::plugin::world::Entity;

enum Callback {
    Void(Arc<dyn Fn() + Send + Sync>),
    Test(Arc<dyn Fn(Test) + Send + Sync>),
    AsyncTest(Arc<dyn Fn(Test) -> BoxFuture<'static, ()> + Send + Sync>),
    BlockPredicate(Arc<dyn Fn(BlockPermutation) -> bool + Send + Sync>),
    EntityPredicate(Arc<dyn Fn(&Entity) -> bool + Send + Sync>),
}

static NEXT_ID: AtomicU32 = AtomicU32::new(0);
static CALLBACKS: Mutex<BTreeMap<u32, Callback>> = Mutex::new(BTreeMap::new());

fn insert(callback: Callback) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    CALLBACKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, callback);
    id
}

fn with<R>(id: u32, f: impl FnOnce(&Callback) -> Option<R>) -> Option<R> {
    CALLBACKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .and_then(f)
}

/// Registers a callback with no arguments (sequence steps, `run-after-delay`, batch hooks, ...).
pub fn void_callback(f: impl Fn() + Send + Sync + 'static) -> VoidCallbackId {
    VoidCallbackId {
        id: insert(Callback::Void(Arc::new(f))),
    }
}

/// Registers a test function for `gametest.register`.
pub fn test_callback(f: impl Fn(Test) + Send + Sync + 'static) -> TestCallbackId {
    TestCallbackId {
        id: insert(Callback::Test(Arc::new(f))),
    }
}

/// Registers an async test function for `gametest.register-async`.
pub fn async_test_callback<F, Fut>(f: F) -> AsyncTestCallbackId
where
    F: Fn(Test) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + 'static,
{
    AsyncTestCallbackId {
        id: insert(Callback::AsyncTest(Arc::new(move |test| Box::pin(f(test))))),
    }
}

/// Registers a block predicate.
pub fn block_predicate(
    f: impl Fn(BlockPermutation) -> bool + Send + Sync + 'static,
) -> BlockPredicateCallbackId {
    BlockPredicateCallbackId {
        id: insert(Callback::BlockPredicate(Arc::new(f))),
    }
}

/// Registers an entity predicate.
pub fn entity_predicate(
    f: impl Fn(&Entity) -> bool + Send + Sync + 'static,
) -> EntityPredicateCallbackId {
    EntityPredicateCallbackId {
        id: insert(Callback::EntityPredicate(Arc::new(f))),
    }
}

/// Drops a registered callback by its id.
pub fn remove(id: u32) {
    CALLBACKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
}

impl crate::wit::exports::pumpkin::plugin::gametest_callbacks::Guest for crate::Component {
    fn invoke_void(callback: VoidCallbackId) {
        if let Some(f) = with(callback.id, |c| match c {
            Callback::Void(f) => Some(Arc::clone(f)),
            _ => None,
        }) {
            f();
        }
    }

    fn invoke_test(callback: TestCallbackId, test: Test) {
        if let Some(f) = with(callback.id, |c| match c {
            Callback::Test(f) => Some(Arc::clone(f)),
            _ => None,
        }) {
            f(test);
        }
    }

    async fn invoke_async_test(callback: AsyncTestCallbackId, test: Test) {
        if let Some(f) = with(callback.id, |c| match c {
            Callback::AsyncTest(f) => Some(Arc::clone(f)),
            _ => None,
        }) {
            f(test).await;
        }
    }

    fn invoke_block_predicate(
        callback: BlockPredicateCallbackId,
        permutation: BlockPermutation,
    ) -> bool {
        with(callback.id, |c| match c {
            Callback::BlockPredicate(f) => Some(Arc::clone(f)),
            _ => None,
        })
        .is_some_and(|f| f(permutation))
    }

    fn invoke_entity_predicate(callback: EntityPredicateCallbackId, entity: &Entity) -> bool {
        with(callback.id, |c| match c {
            Callback::EntityPredicate(f) => Some(Arc::clone(f)),
            _ => None,
        })
        .is_some_and(|f| f(entity))
    }
}
