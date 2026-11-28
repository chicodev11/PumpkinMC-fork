use crate::wit::pumpkin::plugin::event::{EntityPortalReadyEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An entity has stood in a nether portal long enough to travel (Paper `EntityPortalReadyEvent`).
pub struct EntityPortalReadyEvent;
impl FromIntoEvent for EntityPortalReadyEvent {
    const EVENT_TYPE: EventType = EventType::EntityPortalReadyEvent;
    type Data = EntityPortalReadyEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityPortalReadyEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityPortalReadyEvent(data)
    }
}
