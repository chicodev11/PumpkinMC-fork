use crate::wit::pumpkin::plugin::event::{EntityRemoveFromWorldEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `EntityRemoveFromWorldEvent`: an entity left a world. Read-only.
pub struct EntityRemoveFromWorldEvent;
impl FromIntoEvent for EntityRemoveFromWorldEvent {
    const EVENT_TYPE: EventType = EventType::EntityRemoveFromWorldEvent;
    type Data = EntityRemoveFromWorldEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityRemoveFromWorldEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityRemoveFromWorldEvent(data)
    }
}
