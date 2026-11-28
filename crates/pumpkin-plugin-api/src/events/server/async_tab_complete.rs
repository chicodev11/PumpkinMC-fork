use crate::wit::pumpkin::plugin::event::{AsyncTabCompleteEventData, Event, EventType};

use super::super::FromIntoEvent;

/// A player asks for command completions (Paper `AsyncTabCompleteEvent`). A handled event replaces the server's completions.
pub struct AsyncTabCompleteEvent;
impl FromIntoEvent for AsyncTabCompleteEvent {
    const EVENT_TYPE: EventType = EventType::AsyncTabCompleteEvent;
    type Data = AsyncTabCompleteEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::AsyncTabCompleteEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::AsyncTabCompleteEvent(data)
    }
}
