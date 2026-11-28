use crate::wit::pumpkin::plugin::event::{BellRevealRaiderEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `BellRevealRaiderEvent`: a ringing bell makes a raider glow.
pub struct BellRevealRaiderEvent;
impl FromIntoEvent for BellRevealRaiderEvent {
    const EVENT_TYPE: EventType = EventType::BellRevealRaiderEvent;
    type Data = BellRevealRaiderEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::BellRevealRaiderEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::BellRevealRaiderEvent(data)
    }
}
