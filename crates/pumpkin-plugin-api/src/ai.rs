use crate::BoxFuture;
use crate::wit::pumpkin::plugin::context::Server;
use crate::wit::pumpkin::plugin::world::Entity;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Represents a custom entity AI goal for mobs.
///
/// Goal callbacks use shared references so they can be re-entered. Mutable goal state must use
/// thread-safe interior mutability.
#[allow(unused_variables, async_fn_in_trait)]
pub trait AiGoal: Send + Sync {
    /// Returns `true` if the goal should start executing.
    async fn can_start(&self, server: Server, entity: Entity) -> bool {
        false
    }
    /// Returns `true` if the goal should continue executing on subsequent ticks.
    async fn should_continue(&self, server: Server, entity: Entity) -> bool {
        false
    }
    /// Executed when the goal starts.
    async fn start(&self, server: Server, entity: Entity) {}
    /// Executed on every server tick while the goal is active.
    async fn tick(&self, server: Server, entity: Entity) {}
    /// Executed when the goal stops executing.
    async fn stop(&self, server: Server, entity: Entity) {}
}

/// Object-safe form of [`AiGoal`], implemented for every goal type.
#[doc(hidden)]
pub trait ErasedAiGoal: Send + Sync {
    fn can_start_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, bool>;
    fn should_continue_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, bool>;
    fn start_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, ()>;
    fn tick_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, ()>;
    fn stop_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, ()>;
}

impl<G: AiGoal> ErasedAiGoal for G {
    fn can_start_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, bool> {
        Box::pin(self.can_start(server, entity))
    }
    fn should_continue_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, bool> {
        Box::pin(self.should_continue(server, entity))
    }
    fn start_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, ()> {
        Box::pin(self.start(server, entity))
    }
    fn tick_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, ()> {
        Box::pin(self.tick(server, entity))
    }
    fn stop_erased(&self, server: Server, entity: Entity) -> BoxFuture<'_, ()> {
        Box::pin(self.stop(server, entity))
    }
}

pub(crate) static AI_GOAL_HANDLERS: Mutex<LazyAiGoalHandlers> = Mutex::new(LazyAiGoalHandlers {
    handlers: BTreeMap::new(),
    next_id: 0,
});

pub(crate) fn goal(id: u32) -> Option<Arc<dyn ErasedAiGoal>> {
    AI_GOAL_HANDLERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
}

#[allow(dead_code)]
pub(crate) struct LazyAiGoalHandlers {
    pub handlers: BTreeMap<u32, Arc<dyn ErasedAiGoal>>,
    pub next_id: u32,
}

#[allow(dead_code)]
impl LazyAiGoalHandlers {
    #[must_use]
    pub fn register(&mut self, goal: Box<dyn ErasedAiGoal>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.handlers.insert(id, Arc::from(goal));
        id
    }

    #[must_use]
    pub fn get(&self, id: u32) -> Option<Arc<dyn ErasedAiGoal>> {
        self.handlers.get(&id).map(Arc::clone)
    }
}
