use crate::wit::pumpkin::plugin::event::{Event, EventType, SulfurCubeSwallowItemEventData};

use super::super::FromIntoEvent;

/// A sulfur cube swallows an item from a player (Paper `SulfurCubeSwallowItemEvent`).
pub struct SulfurCubeSwallowItemEvent;
impl FromIntoEvent for SulfurCubeSwallowItemEvent {
    const EVENT_TYPE: EventType = EventType::SulfurCubeSwallowItemEvent;
    type Data = SulfurCubeSwallowItemEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::SulfurCubeSwallowItemEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::SulfurCubeSwallowItemEvent(data)
    }
}
