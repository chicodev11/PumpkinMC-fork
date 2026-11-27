use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerUntrackEntityEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerUntrackEntityEvent`: a player stopped tracking an entity. Read-only.
pub struct PlayerUntrackEntityEvent;
impl FromIntoEvent for PlayerUntrackEntityEvent {
    const EVENT_TYPE: EventType = EventType::PlayerUntrackEntityEvent;
    type Data = PlayerUntrackEntityEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerUntrackEntityEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerUntrackEntityEvent(data)
    }
}
