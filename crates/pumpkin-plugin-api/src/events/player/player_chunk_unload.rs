use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerChunkUnloadEventData};

use super::super::FromIntoEvent;

/// A chunk is unloaded for a player (Paper `PlayerChunkUnloadEvent`).
pub struct PlayerChunkUnloadEvent;
impl FromIntoEvent for PlayerChunkUnloadEvent {
    const EVENT_TYPE: EventType = EventType::PlayerChunkUnloadEvent;
    type Data = PlayerChunkUnloadEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerChunkUnloadEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerChunkUnloadEvent(data)
    }
}
