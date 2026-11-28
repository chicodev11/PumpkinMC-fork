use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerLecternPageChangeEventData};

use super::super::FromIntoEvent;

/// A player turns a page of a book on a lectern (Paper `PlayerLecternPageChangeEvent`).
pub struct PlayerLecternPageChangeEvent;
impl FromIntoEvent for PlayerLecternPageChangeEvent {
    const EVENT_TYPE: EventType = EventType::PlayerLecternPageChangeEvent;
    type Data = PlayerLecternPageChangeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerLecternPageChangeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerLecternPageChangeEvent(data)
    }
}
