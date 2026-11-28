use crate::wit::pumpkin::plugin::event::{EntityIgniteEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An entity starts its fuse (Paper `EntityIgniteEvent`).
pub struct EntityIgniteEvent;
impl FromIntoEvent for EntityIgniteEvent {
    const EVENT_TYPE: EventType = EventType::EntityIgniteEvent;
    type Data = EntityIgniteEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityIgniteEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityIgniteEvent(data)
    }
}
