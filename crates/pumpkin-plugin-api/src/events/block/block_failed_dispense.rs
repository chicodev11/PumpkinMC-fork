use crate::wit::pumpkin::plugin::event::{BlockFailedDispenseEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `BlockFailedDispenseEvent`: a dispenser or dropper was powered while empty.
pub struct BlockFailedDispenseEvent;
impl FromIntoEvent for BlockFailedDispenseEvent {
    const EVENT_TYPE: EventType = EventType::BlockFailedDispenseEvent;
    type Data = BlockFailedDispenseEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::BlockFailedDispenseEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::BlockFailedDispenseEvent(data)
    }
}
