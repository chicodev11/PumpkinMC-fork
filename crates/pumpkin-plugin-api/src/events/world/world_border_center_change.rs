use crate::wit::pumpkin::plugin::event::{Event, EventType, WorldBorderCenterChangeEventData};

use super::super::FromIntoEvent;

/// A world border moves its center (Paper `WorldBorderCenterChangeEvent`).
pub struct WorldBorderCenterChangeEvent;
impl FromIntoEvent for WorldBorderCenterChangeEvent {
    const EVENT_TYPE: EventType = EventType::WorldBorderCenterChangeEvent;
    type Data = WorldBorderCenterChangeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::WorldBorderCenterChangeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::WorldBorderCenterChangeEvent(data)
    }
}
