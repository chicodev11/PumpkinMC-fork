use crate::wit::pumpkin::plugin::event::{CreeperIgniteEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `CreeperIgniteEvent`: a creeper is lit with flint and steel.
pub struct CreeperIgniteEvent;
impl FromIntoEvent for CreeperIgniteEvent {
    const EVENT_TYPE: EventType = EventType::CreeperIgniteEvent;
    type Data = CreeperIgniteEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::CreeperIgniteEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::CreeperIgniteEvent(data)
    }
}
