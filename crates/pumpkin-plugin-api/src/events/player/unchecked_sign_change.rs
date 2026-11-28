use crate::wit::pumpkin::plugin::event::{Event, EventType, UncheckedSignChangeEventData};

use super::super::FromIntoEvent;

/// A player edits a sign that is not in the world (Paper `UncheckedSignChangeEvent`).
pub struct UncheckedSignChangeEvent;
impl FromIntoEvent for UncheckedSignChangeEvent {
    const EVENT_TYPE: EventType = EventType::UncheckedSignChangeEvent;
    type Data = UncheckedSignChangeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::UncheckedSignChangeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::UncheckedSignChangeEvent(data)
    }
}
