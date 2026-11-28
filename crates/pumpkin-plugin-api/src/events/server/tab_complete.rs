use crate::wit::pumpkin::plugin::event::{Event, EventType, TabCompleteEventData};

use super::super::FromIntoEvent;

/// Command completions are about to be sent (Paper `TabCompleteEvent`).
pub struct TabCompleteEvent;
impl FromIntoEvent for TabCompleteEvent {
    const EVENT_TYPE: EventType = EventType::TabCompleteEvent;
    type Data = TabCompleteEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::TabCompleteEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::TabCompleteEvent(data)
    }
}
