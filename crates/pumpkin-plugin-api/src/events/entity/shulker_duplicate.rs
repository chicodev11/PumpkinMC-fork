use crate::wit::pumpkin::plugin::event::{Event, EventType, ShulkerDuplicateEventData};

use super::super::FromIntoEvent;

/// A shulker hit by a shulker bullet duplicates (Paper `ShulkerDuplicateEvent`).
pub struct ShulkerDuplicateEvent;
impl FromIntoEvent for ShulkerDuplicateEvent {
    const EVENT_TYPE: EventType = EventType::ShulkerDuplicateEvent;
    type Data = ShulkerDuplicateEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::ShulkerDuplicateEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::ShulkerDuplicateEvent(data)
    }
}
