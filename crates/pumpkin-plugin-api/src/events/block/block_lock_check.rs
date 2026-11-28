use crate::wit::pumpkin::plugin::event::{BlockLockCheckEventData, Event, EventType};

use super::super::FromIntoEvent;

/// A player opens a locked container (Paper `BlockLockCheckEvent`). `result` is `DEFAULT`, `ALLOW` or `DENY`.
pub struct BlockLockCheckEvent;
impl FromIntoEvent for BlockLockCheckEvent {
    const EVENT_TYPE: EventType = EventType::BlockLockCheckEvent;
    type Data = BlockLockCheckEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::BlockLockCheckEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::BlockLockCheckEvent(data)
    }
}
