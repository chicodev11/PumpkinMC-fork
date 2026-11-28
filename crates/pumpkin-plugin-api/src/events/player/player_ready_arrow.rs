use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerReadyArrowEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerReadyArrowEvent`: a bow or crossbow picks an arrow. Cancel to skip it (the next is tried).
pub struct PlayerReadyArrowEvent;
impl FromIntoEvent for PlayerReadyArrowEvent {
    const EVENT_TYPE: EventType = EventType::PlayerReadyArrowEvent;
    type Data = PlayerReadyArrowEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerReadyArrowEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerReadyArrowEvent(data)
    }
}
