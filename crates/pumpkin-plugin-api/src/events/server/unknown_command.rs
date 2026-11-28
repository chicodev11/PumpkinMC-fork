use crate::wit::pumpkin::plugin::event::{Event, EventType, UnknownCommandEventData};

use super::super::FromIntoEvent;

/// Someone runs a command that does not exist (Paper `UnknownCommandEvent`). No message shows nothing.
pub struct UnknownCommandEvent;
impl FromIntoEvent for UnknownCommandEvent {
    const EVENT_TYPE: EventType = EventType::UnknownCommandEvent;
    type Data = UnknownCommandEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::UnknownCommandEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::UnknownCommandEvent(data)
    }
}
