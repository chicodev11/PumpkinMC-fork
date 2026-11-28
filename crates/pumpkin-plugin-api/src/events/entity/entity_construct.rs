use crate::wit::pumpkin::plugin::event::{EntityConstructEventData, Event, EventType};

use super::super::FromIntoEvent;

/// A golem or wither is built from blocks (Paper `EntityConstructEvent`).
pub struct EntityConstructEvent;
impl FromIntoEvent for EntityConstructEvent {
    const EVENT_TYPE: EventType = EventType::EntityConstructEvent;
    type Data = EntityConstructEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityConstructEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityConstructEvent(data)
    }
}
