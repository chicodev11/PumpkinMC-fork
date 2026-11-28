use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerStartSpectatingEntityEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerStartSpectatingEntityEvent`: a spectator starts viewing through an entity.
pub struct PlayerStartSpectatingEntityEvent;
impl FromIntoEvent for PlayerStartSpectatingEntityEvent {
    const EVENT_TYPE: EventType = EventType::PlayerStartSpectatingEntityEvent;
    type Data = PlayerStartSpectatingEntityEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerStartSpectatingEntityEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerStartSpectatingEntityEvent(data)
    }
}
