use crate::wit::pumpkin::plugin::event::{Event, EventType, PufferFishStateChangeEventData};

use super::super::FromIntoEvent;

/// Paper `PufferFishStateChangeEvent`: a pufferfish puffs up or down (0 small, 1 mid, 2 full).
pub struct PufferFishStateChangeEvent;
impl FromIntoEvent for PufferFishStateChangeEvent {
    const EVENT_TYPE: EventType = EventType::PufferFishStateChangeEvent;
    type Data = PufferFishStateChangeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PufferFishStateChangeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PufferFishStateChangeEvent(data)
    }
}
