use crate::wit::pumpkin::plugin::event::{AsyncPlayerSendSuggestionsEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Command suggestions are sent to a player (Paper `AsyncPlayerSendSuggestionsEvent`).
pub struct AsyncPlayerSendSuggestionsEvent;
impl FromIntoEvent for AsyncPlayerSendSuggestionsEvent {
    const EVENT_TYPE: EventType = EventType::AsyncPlayerSendSuggestionsEvent;
    type Data = AsyncPlayerSendSuggestionsEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::AsyncPlayerSendSuggestionsEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::AsyncPlayerSendSuggestionsEvent(data)
    }
}
