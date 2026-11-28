use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerChunkLoadEventData};

use super::super::FromIntoEvent;

/// A chunk is sent to a player (Paper `PlayerChunkLoadEvent`).
pub struct PlayerChunkLoadEvent;
impl FromIntoEvent for PlayerChunkLoadEvent {
    const EVENT_TYPE: EventType = EventType::PlayerChunkLoadEvent;
    type Data = PlayerChunkLoadEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerChunkLoadEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerChunkLoadEvent(data)
    }
}
