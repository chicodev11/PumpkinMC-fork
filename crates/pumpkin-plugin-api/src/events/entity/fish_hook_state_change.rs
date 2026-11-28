use crate::wit::pumpkin::plugin::event::{Event, EventType, FishHookStateChangeEventData};

use super::super::FromIntoEvent;

/// A fishing hook changes state (Paper `FishHookStateChangeEvent`).
pub struct FishHookStateChangeEvent;
impl FromIntoEvent for FishHookStateChangeEvent {
    const EVENT_TYPE: EventType = EventType::FishHookStateChangeEvent;
    type Data = FishHookStateChangeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::FishHookStateChangeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::FishHookStateChangeEvent(data)
    }
}
