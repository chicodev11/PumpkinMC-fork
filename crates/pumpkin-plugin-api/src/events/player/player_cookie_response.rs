use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerCookieResponseEventData};

use super::super::FromIntoEvent;

/// A Java client's answer to `request-cookie`.
pub struct PlayerCookieResponseEvent;

impl FromIntoEvent for PlayerCookieResponseEvent {
    const EVENT_TYPE: EventType = EventType::PlayerCookieResponseEvent;
    type Data = PlayerCookieResponseEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerCookieResponseEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerCookieResponseEvent(data)
    }
}
