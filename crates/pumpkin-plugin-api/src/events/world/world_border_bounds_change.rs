use crate::wit::pumpkin::plugin::event::{Event, EventType, WorldBorderBoundsChangeEventData};

use super::super::FromIntoEvent;

/// A world border changes size (Paper `WorldBorderBoundsChangeEvent`).
pub struct WorldBorderBoundsChangeEvent;
impl FromIntoEvent for WorldBorderBoundsChangeEvent {
    const EVENT_TYPE: EventType = EventType::WorldBorderBoundsChangeEvent;
    type Data = WorldBorderBoundsChangeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::WorldBorderBoundsChangeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::WorldBorderBoundsChangeEvent(data)
    }
}
