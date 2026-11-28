use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerBedFailEnterEventData};

use super::super::FromIntoEvent;

/// A player fails to get into a bed (Paper `PlayerBedFailEnterEvent`). Cancelling skips the failure message or explosion.
pub struct PlayerBedFailEnterEvent;
impl FromIntoEvent for PlayerBedFailEnterEvent {
    const EVENT_TYPE: EventType = EventType::PlayerBedFailEnterEvent;
    type Data = PlayerBedFailEnterEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerBedFailEnterEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerBedFailEnterEvent(data)
    }
}
