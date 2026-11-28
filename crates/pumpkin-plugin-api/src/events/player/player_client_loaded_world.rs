use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerClientLoadedWorldEventData};

use super::super::FromIntoEvent;

/// A player's client finished loading the world, or the wait for it ran out (Paper `PlayerClientLoadedWorldEvent`).
pub struct PlayerClientLoadedWorldEvent;
impl FromIntoEvent for PlayerClientLoadedWorldEvent {
    const EVENT_TYPE: EventType = EventType::PlayerClientLoadedWorldEvent;
    type Data = PlayerClientLoadedWorldEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerClientLoadedWorldEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerClientLoadedWorldEvent(data)
    }
}
