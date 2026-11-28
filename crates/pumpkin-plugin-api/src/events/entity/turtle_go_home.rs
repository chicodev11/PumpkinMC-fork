use crate::wit::pumpkin::plugin::event::{Event, EventType, TurtleGoHomeEventData};

use super::super::FromIntoEvent;

/// A turtle heads home (Paper `TurtleGoHomeEvent`).
pub struct TurtleGoHomeEvent;
impl FromIntoEvent for TurtleGoHomeEvent {
    const EVENT_TYPE: EventType = EventType::TurtleGoHomeEvent;
    type Data = TurtleGoHomeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::TurtleGoHomeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::TurtleGoHomeEvent(data)
    }
}
