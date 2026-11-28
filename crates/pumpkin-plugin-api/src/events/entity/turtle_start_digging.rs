use crate::wit::pumpkin::plugin::event::{Event, EventType, TurtleStartDiggingEventData};

use super::super::FromIntoEvent;

/// A turtle starts digging to lay eggs (Paper `TurtleStartDiggingEvent`).
pub struct TurtleStartDiggingEvent;
impl FromIntoEvent for TurtleStartDiggingEvent {
    const EVENT_TYPE: EventType = EventType::TurtleStartDiggingEvent;
    type Data = TurtleStartDiggingEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::TurtleStartDiggingEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::TurtleStartDiggingEvent(data)
    }
}
