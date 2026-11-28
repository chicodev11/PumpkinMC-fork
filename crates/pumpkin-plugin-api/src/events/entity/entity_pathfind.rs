use crate::wit::pumpkin::plugin::event::{EntityPathfindEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `EntityPathfindEvent`: a mob starts finding a path to `location`.
pub struct EntityPathfindEvent;
impl FromIntoEvent for EntityPathfindEvent {
    const EVENT_TYPE: EventType = EventType::EntityPathfindEvent;
    type Data = EntityPathfindEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityPathfindEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityPathfindEvent(data)
    }
}
