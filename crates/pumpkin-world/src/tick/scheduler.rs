use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use pumpkin_util::math::position::BlockPos;
use rustc_hash::FxHashSet;

use crate::tick::{MAX_TICK_DELAY, OrderedTick, ScheduledTick};

pub struct ChunkTickScheduler<T> {
    inner: Mutex<Option<Box<ChunkTickSchedulerInner<T>>>>,
    offset: AtomicUsize,
}

struct ChunkTickSchedulerInner<T> {
    tick_queue: [Vec<OrderedTick<T>>; MAX_TICK_DELAY],
    queued_ticks: FxHashSet<(BlockPos, T)>,
}

impl<'a, T: std::hash::Hash + Eq> ChunkTickScheduler<&'a T> {
    pub fn step_tick(&self) -> Vec<OrderedTick<&'a T>> {
        // Atomic update for the offset
        let current_offset = self.offset.fetch_add(1, Ordering::SeqCst) % MAX_TICK_DELAY;
        let next_offset = (current_offset + 1) % MAX_TICK_DELAY;
        self.offset.store(next_offset, Ordering::SeqCst);

        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_mut() else {
            return Vec::new();
        };

        let res = std::mem::take(&mut inner.tick_queue[current_offset]);

        if !res.is_empty() {
            for next_tick in &res {
                inner
                    .queued_ticks
                    .remove(&(next_tick.position, next_tick.value));
            }
            if inner.queued_ticks.is_empty() {
                *inner_guard = None;
            }
        }
        res
    }

    pub fn schedule_tick(&self, tick: &ScheduledTick<&'a T>, sub_tick_order: u64) {
        let offset = self.offset.load(Ordering::SeqCst);
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let inner = inner_guard.get_or_insert_with(|| {
            Box::new(ChunkTickSchedulerInner {
                tick_queue: std::array::from_fn(|_| Vec::new()),
                queued_ticks: FxHashSet::default(),
            })
        });

        if inner.queued_ticks.insert((tick.position, tick.value)) {
            // `offset` is the slot the next `step_tick` runs. Vanilla schedules for
            // `gameTime + delay` and advances the time before running ticks, so a tick with
            // delay `d` runs on the `d`-th step from now (delay 0 runs on the next one).
            let index = (offset + (tick.delay as usize).max(1) - 1) % MAX_TICK_DELAY;

            inner.tick_queue[index].push(OrderedTick {
                priority: tick.priority,
                sub_tick_order,
                position: tick.position,
                value: tick.value,
            });
        }
    }

    pub fn is_scheduled(&self, pos: BlockPos, value: &T) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|inner| inner.queued_ticks.contains(&(pos, value)))
    }

    pub fn clear_area(&self, min: &BlockPos, max: &BlockPos) {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_mut() else {
            return;
        };

        let contains = |position: &BlockPos| {
            position.0.x >= min.0.x
                && position.0.x < max.0.x
                && position.0.y >= min.0.y
                && position.0.y < max.0.y
                && position.0.z >= min.0.z
                && position.0.z < max.0.z
        };

        for queue in &mut inner.tick_queue {
            queue.retain(|tick| !contains(&tick.position));
        }
        inner
            .queued_ticks
            .retain(|(position, _)| !contains(position));
        let became_empty = inner.queued_ticks.is_empty();

        if became_empty {
            *inner_guard = None;
        }
    }

    pub fn has_ticks(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|inner| !inner.queued_ticks.is_empty())
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<ScheduledTick<&'a T>> {
        let offset = self.offset.load(Ordering::SeqCst);
        let inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_ref() else {
            return Vec::new();
        };

        let mut res = Vec::new();

        for i in 0..MAX_TICK_DELAY {
            let index = (offset + i) % MAX_TICK_DELAY;
            // Slot `offset + i` runs on step `i + 1`: vanilla's saved `t`.
            res.extend(inner.tick_queue[index].iter().map(|x| ScheduledTick {
                delay: (i + 1).min(u8::MAX as usize) as u8,
                priority: x.priority,
                position: x.position,
                value: x.value,
            }));
        }
        res
    }
}

impl<'a, T: std::hash::Hash + Eq + 'static> FromIterator<ScheduledTick<&'a T>>
    for ChunkTickScheduler<&'a T>
{
    fn from_iter<I: IntoIterator<Item = ScheduledTick<&'a T>>>(iter: I) -> Self {
        let scheduler = Self::default();
        let iter = iter.into_iter();

        let (lower, _) = iter.size_hint();
        if lower > 0 {
            let mut inner_guard = scheduler
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let inner = inner_guard.get_or_insert_with(|| {
                Box::new(ChunkTickSchedulerInner {
                    tick_queue: std::array::from_fn(|_| Vec::new()),
                    queued_ticks: FxHashSet::default(),
                })
            });
            inner.queued_ticks.reserve(lower);
        }

        for tick in iter {
            scheduler.schedule_tick(&tick, 0);
        }
        scheduler
    }
}

impl<T> Default for ChunkTickScheduler<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            offset: AtomicUsize::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ChunkTickScheduler;
    use crate::tick::{ScheduledTick, TickPriority};
    use pumpkin_util::math::position::BlockPos;

    static VALUE: u8 = 0;

    fn schedule(scheduler: &ChunkTickScheduler<&'static u8>, x: i32, delay: u8) {
        scheduler.schedule_tick(
            &ScheduledTick {
                delay,
                priority: TickPriority::Normal,
                position: BlockPos::new(x, 0, 0),
                value: &VALUE,
            },
            0,
        );
    }

    /// Vanilla runs a tick scheduled with delay `d` on the `d`-th tick from now (a repeater
    /// with delay 1 turns on 2 ticks after its input).
    #[test]
    fn delay_counts_steps_from_now() {
        let scheduler = ChunkTickScheduler::default();
        schedule(&scheduler, 0, 0);
        schedule(&scheduler, 1, 1);
        schedule(&scheduler, 2, 2);
        let ran = |s: &ChunkTickScheduler<&'static u8>| {
            s.step_tick()
                .iter()
                .map(|t| t.position.0.x)
                .collect::<Vec<_>>()
        };
        assert_eq!(ran(&scheduler), [0, 1]);
        // Scheduled while the next step is pending, like a tick scheduled during a tick
        schedule(&scheduler, 3, 1);
        assert_eq!(ran(&scheduler), [2, 3]);
        assert!(ran(&scheduler).is_empty());
    }

    #[test]
    fn saved_delay_round_trips() {
        let scheduler = ChunkTickScheduler::default();
        schedule(&scheduler, 0, 3);
        let saved = scheduler.to_vec();
        assert_eq!(saved[0].delay, 3);
        let loaded: ChunkTickScheduler<&'static u8> = saved.into_iter().collect();
        assert!(loaded.step_tick().is_empty());
        assert!(loaded.step_tick().is_empty());
        assert_eq!(loaded.step_tick().len(), 1);
    }
}
