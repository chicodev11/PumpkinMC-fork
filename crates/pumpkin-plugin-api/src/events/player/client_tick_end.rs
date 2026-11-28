use crate::wit::pumpkin::plugin::event::{ClientTickEndEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `ClientTickEndEvent`: the client finished a tick. Keep handlers quick.
pub struct ClientTickEndEvent;
impl FromIntoEvent for ClientTickEndEvent {
    const EVENT_TYPE: EventType = EventType::ClientTickEndEvent;
    type Data = ClientTickEndEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::ClientTickEndEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::ClientTickEndEvent(data)
    }
}
