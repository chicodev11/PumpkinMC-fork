use crate::wit::pumpkin::plugin::event::{Event, EventType, PhantomPreSpawnEventData};

use super::super::FromIntoEvent;

/// A phantom is about to spawn for a player (Paper `PhantomPreSpawnEvent`).
pub struct PhantomPreSpawnEvent;
impl FromIntoEvent for PhantomPreSpawnEvent {
    const EVENT_TYPE: EventType = EventType::PhantomPreSpawnEvent;
    type Data = PhantomPreSpawnEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PhantomPreSpawnEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PhantomPreSpawnEvent(data)
    }
}
