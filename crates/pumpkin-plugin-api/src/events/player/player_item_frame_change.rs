use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerItemFrameChangeEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerItemFrameChangeEvent`: a player puts an item into (`PLACE`) or turns (`ROTATE`) an item frame.
pub struct PlayerItemFrameChangeEvent;
impl FromIntoEvent for PlayerItemFrameChangeEvent {
    const EVENT_TYPE: EventType = EventType::PlayerItemFrameChangeEvent;
    type Data = PlayerItemFrameChangeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerItemFrameChangeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerItemFrameChangeEvent(data)
    }
}
