use crate::wit::pumpkin::plugin::event::{BlockDestroyEventData, Event, EventType};

use super::super::FromIntoEvent;

/// A block is destroyed without a player, for example by physics (Paper `BlockDestroyEvent`).
pub struct BlockDestroyEvent;
impl FromIntoEvent for BlockDestroyEvent {
    const EVENT_TYPE: EventType = EventType::BlockDestroyEvent;
    type Data = BlockDestroyEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::BlockDestroyEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::BlockDestroyEvent(data)
    }
}
