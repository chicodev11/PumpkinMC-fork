use std::sync::{Arc, atomic::Ordering};

use pumpkin_util::math::position::BlockPos;

use crate::{block::entities::BlockEntity, world::World};

pub use pumpkin_inventory::ViewerCountTracker;

pub trait ViewerCountTrackerExt {
    fn update_viewer_count<T>(&self, entity: &T, world: &Arc<World>, position: &BlockPos)
    where
        T: BlockEntity + ViewerCountListener + 'static,
    {
        self.update_viewer_count_by(entity, world, position, None);
    }

    /// Applies a change in viewers; `user` is the player who just opened or closed it.
    fn update_viewer_count_by<T>(
        &self,
        entity: &T,
        world: &Arc<World>,
        position: &BlockPos,
        user: Option<&dyn crate::entity::EntityBase>,
    ) where
        T: BlockEntity + ViewerCountListener + 'static;
}

impl ViewerCountTrackerExt for ViewerCountTracker {
    fn update_viewer_count_by<T>(
        &self,
        entity: &T,
        world: &Arc<World>,
        position: &BlockPos,
        user: Option<&dyn crate::entity::EntityBase>,
    ) where
        T: BlockEntity + ViewerCountListener + 'static,
    {
        let current = self.current.load(Ordering::Relaxed);
        let old = self.old.swap(current, Ordering::Relaxed);
        if old != current {
            match (old, current) {
                // Vanilla `ContainerOpenersCounter`: sculk sensors hear containers open and
                // close, credited to the player who did it.
                (n, 0) if n > 0 => {
                    entity.on_container_close(world, position);
                    world.emit_game_event_with_context(
                        pumpkin_data::game_event::GameEvent::ContainerClose.name(),
                        position.to_centered_f64(),
                        &crate::world::game_event::GameEventContext {
                            source_entity: user,
                        },
                    );
                }
                (0, n) if n > 0 => {
                    entity.on_container_open(world, position);
                    // The player who opened it is the one with this container open
                    let found = world
                        .players
                        .load()
                        .iter()
                        .find(|player| player.open_container_pos.load() == Some(*position))
                        .cloned();
                    world.emit_game_event_with_context(
                        pumpkin_data::game_event::GameEvent::ContainerOpen.name(),
                        position.to_centered_f64(),
                        &crate::world::game_event::GameEventContext {
                            source_entity: user.or_else(|| {
                                found
                                    .as_deref()
                                    .map(|player| player as &dyn crate::entity::EntityBase)
                            }),
                        },
                    );
                }
                _ => {} // Ignore
            }

            entity.on_viewer_count_update(world, position, old, current);
        }
    }
}

pub trait ViewerCountListener: Send + Sync {
    fn on_container_open(&self, _world: &Arc<World>, _position: &BlockPos) {}

    fn on_container_close(&self, _world: &Arc<World>, _position: &BlockPos) {}

    fn on_viewer_count_update(
        &self,
        _world: &Arc<World>,
        _position: &BlockPos,
        _old: u16,
        _new: u16,
    ) {
    }
}
