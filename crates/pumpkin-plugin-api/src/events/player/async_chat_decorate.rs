use crate::wit::pumpkin::plugin::event::{AsyncChatDecorateEventData, Event, EventType};

use super::super::FromIntoEvent;

/// A chat message is decorated (Paper `AsyncChatDecorateEvent`).
pub struct AsyncChatDecorateEvent;
impl FromIntoEvent for AsyncChatDecorateEvent {
    const EVENT_TYPE: EventType = EventType::AsyncChatDecorateEvent;
    type Data = AsyncChatDecorateEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::AsyncChatDecorateEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::AsyncChatDecorateEvent(data)
    }
}
