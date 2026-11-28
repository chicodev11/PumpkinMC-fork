use crate::wit::pumpkin::plugin::event::{Event, EventType, SlimeWanderEventData};

use super::super::FromIntoEvent;

/// A slime wanders (Paper `SlimeWanderEvent`).
pub struct SlimeWanderEvent;
impl FromIntoEvent for SlimeWanderEvent {
    const EVENT_TYPE: EventType = EventType::SlimeWanderEvent;
    type Data = SlimeWanderEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::SlimeWanderEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::SlimeWanderEvent(data)
    }
}
