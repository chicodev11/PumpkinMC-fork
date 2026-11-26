use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerPickBlockEventData};

use super::super::FromIntoEvent;

/// An event fired when a player picks a block (middle click). Plugins may change or cancel the picked item.
pub struct PlayerPickBlockEvent;

impl FromIntoEvent for PlayerPickBlockEvent {
    const EVENT_TYPE: EventType = EventType::PlayerPickBlockEvent;
    type Data = PlayerPickBlockEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerPickBlockEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerPickBlockEvent(data)
    }
}
