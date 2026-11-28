use crate::wit::pumpkin::plugin::event::{BlockBreakBlockEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Flowing fluid washes a block away (Paper `BlockBreakBlockEvent`).
pub struct BlockBreakBlockEvent;
impl FromIntoEvent for BlockBreakBlockEvent {
    const EVENT_TYPE: EventType = EventType::BlockBreakBlockEvent;
    type Data = BlockBreakBlockEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::BlockBreakBlockEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::BlockBreakBlockEvent(data)
    }
}
