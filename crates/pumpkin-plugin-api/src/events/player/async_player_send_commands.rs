use crate::wit::pumpkin::plugin::event::{AsyncPlayerSendCommandsEventData, Event, EventType};

use super::super::FromIntoEvent;

/// The command tree is sent to a player (Paper `AsyncPlayerSendCommandsEvent`). Removing a root command hides it from the client.
pub struct AsyncPlayerSendCommandsEvent;
impl FromIntoEvent for AsyncPlayerSendCommandsEvent {
    const EVENT_TYPE: EventType = EventType::AsyncPlayerSendCommandsEvent;
    type Data = AsyncPlayerSendCommandsEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::AsyncPlayerSendCommandsEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::AsyncPlayerSendCommandsEvent(data)
    }
}
