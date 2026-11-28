use crate::wit::pumpkin::plugin::event::{Event, EventType, TurtleLayEggEventData};

use super::super::FromIntoEvent;

/// A turtle lays eggs (Paper `TurtleLayEggEvent`).
pub struct TurtleLayEggEvent;
impl FromIntoEvent for TurtleLayEggEvent {
    const EVENT_TYPE: EventType = EventType::TurtleLayEggEvent;
    type Data = TurtleLayEggEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::TurtleLayEggEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::TurtleLayEggEvent(data)
    }
}
