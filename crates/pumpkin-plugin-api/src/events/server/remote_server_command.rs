use crate::wit::pumpkin::plugin::event::{Event, EventType, RemoteServerCommandEventData};

use super::super::FromIntoEvent;

/// A command arrives over RCON (Paper `RemoteServerCommandEvent`).
pub struct RemoteServerCommandEvent;
impl FromIntoEvent for RemoteServerCommandEvent {
    const EVENT_TYPE: EventType = EventType::RemoteServerCommandEvent;
    type Data = RemoteServerCommandEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::RemoteServerCommandEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::RemoteServerCommandEvent(data)
    }
}
