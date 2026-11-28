use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerInsertLecternBookEventData};

use super::super::FromIntoEvent;

/// A player puts a book on a lectern (Paper `PlayerInsertLecternBookEvent`).
pub struct PlayerInsertLecternBookEvent;
impl FromIntoEvent for PlayerInsertLecternBookEvent {
    const EVENT_TYPE: EventType = EventType::PlayerInsertLecternBookEvent;
    type Data = PlayerInsertLecternBookEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerInsertLecternBookEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerInsertLecternBookEvent(data)
    }
}
