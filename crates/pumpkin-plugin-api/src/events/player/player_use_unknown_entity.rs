use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerUseUnknownEntityEventData};

use super::super::FromIntoEvent;

/// A player interacts with an entity the server does not know (Paper `PlayerUseUnknownEntityEvent`).
pub struct PlayerUseUnknownEntityEvent;
impl FromIntoEvent for PlayerUseUnknownEntityEvent {
    const EVENT_TYPE: EventType = EventType::PlayerUseUnknownEntityEvent;
    type Data = PlayerUseUnknownEntityEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerUseUnknownEntityEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerUseUnknownEntityEvent(data)
    }
}
