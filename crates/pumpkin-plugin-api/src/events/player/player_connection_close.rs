use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerConnectionCloseEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerConnectionCloseEvent`: a joined player's connection closed. Read-only.
pub struct PlayerConnectionCloseEvent;
impl FromIntoEvent for PlayerConnectionCloseEvent {
    const EVENT_TYPE: EventType = EventType::PlayerConnectionCloseEvent;
    type Data = PlayerConnectionCloseEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerConnectionCloseEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerConnectionCloseEvent(data)
    }
}
