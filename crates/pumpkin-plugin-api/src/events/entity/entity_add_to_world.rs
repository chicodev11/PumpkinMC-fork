use crate::wit::pumpkin::plugin::event::{EntityAddToWorldEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `EntityAddToWorldEvent`: an entity was added to a world. Read-only.
pub struct EntityAddToWorldEvent;
impl FromIntoEvent for EntityAddToWorldEvent {
    const EVENT_TYPE: EventType = EventType::EntityAddToWorldEvent;
    type Data = EntityAddToWorldEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityAddToWorldEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityAddToWorldEvent(data)
    }
}
