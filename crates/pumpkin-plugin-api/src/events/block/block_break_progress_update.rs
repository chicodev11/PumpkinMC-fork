use crate::wit::pumpkin::plugin::event::{BlockBreakProgressUpdateEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `BlockBreakProgressUpdateEvent`: the crack progress (0-1) shown on a block changed.
pub struct BlockBreakProgressUpdateEvent;
impl FromIntoEvent for BlockBreakProgressUpdateEvent {
    const EVENT_TYPE: EventType = EventType::BlockBreakProgressUpdateEvent;
    type Data = BlockBreakProgressUpdateEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::BlockBreakProgressUpdateEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::BlockBreakProgressUpdateEvent(data)
    }
}
