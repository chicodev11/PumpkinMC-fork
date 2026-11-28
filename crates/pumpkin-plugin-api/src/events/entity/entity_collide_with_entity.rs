use crate::wit::pumpkin::plugin::event::{EntityCollideWithEntityEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An entity pushes into another (Paper `EntityCollideWithEntityEvent`).
pub struct EntityCollideWithEntityEvent;
impl FromIntoEvent for EntityCollideWithEntityEvent {
    const EVENT_TYPE: EventType = EventType::EntityCollideWithEntityEvent;
    type Data = EntityCollideWithEntityEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityCollideWithEntityEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityCollideWithEntityEvent(data)
    }
}
