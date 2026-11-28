use crate::wit::pumpkin::plugin::event::{Event, EventType, SlimeSwimEventData};

use super::super::FromIntoEvent;

/// A slime swims (Paper `SlimeSwimEvent`).
pub struct SlimeSwimEvent;
impl FromIntoEvent for SlimeSwimEvent {
    const EVENT_TYPE: EventType = EventType::SlimeSwimEvent;
    type Data = SlimeSwimEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::SlimeSwimEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::SlimeSwimEvent(data)
    }
}
