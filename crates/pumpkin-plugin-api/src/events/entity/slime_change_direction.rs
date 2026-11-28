use crate::wit::pumpkin::plugin::event::{Event, EventType, SlimeChangeDirectionEventData};

use super::super::FromIntoEvent;

/// A slime picks a new direction (Paper `SlimeChangeDirectionEvent`).
pub struct SlimeChangeDirectionEvent;
impl FromIntoEvent for SlimeChangeDirectionEvent {
    const EVENT_TYPE: EventType = EventType::SlimeChangeDirectionEvent;
    type Data = SlimeChangeDirectionEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::SlimeChangeDirectionEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::SlimeChangeDirectionEvent(data)
    }
}
