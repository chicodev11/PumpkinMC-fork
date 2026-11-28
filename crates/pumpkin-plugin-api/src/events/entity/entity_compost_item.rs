use crate::wit::pumpkin::plugin::event::{EntityCompostItemEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An entity puts an item into a composter (Paper `EntityCompostItemEvent`).
pub struct EntityCompostItemEvent;
impl FromIntoEvent for EntityCompostItemEvent {
    const EVENT_TYPE: EventType = EventType::EntityCompostItemEvent;
    type Data = EntityCompostItemEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityCompostItemEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityCompostItemEvent(data)
    }
}
