use crate::wit::pumpkin::plugin::event::{BlockPreDispenseEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `BlockPreDispenseEvent`: a dispenser picked the item in `slot` and is about to dispense it.
pub struct BlockPreDispenseEvent;
impl FromIntoEvent for BlockPreDispenseEvent {
    const EVENT_TYPE: EventType = EventType::BlockPreDispenseEvent;
    type Data = BlockPreDispenseEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::BlockPreDispenseEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::BlockPreDispenseEvent(data)
    }
}
