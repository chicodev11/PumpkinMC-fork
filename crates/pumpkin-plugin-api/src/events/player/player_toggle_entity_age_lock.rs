use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerToggleEntityAgeLockEventData};

use super::super::FromIntoEvent;

/// A player locks or unlocks the age of an entity (Paper `PlayerToggleEntityAgeLockEvent`).
pub struct PlayerToggleEntityAgeLockEvent;
impl FromIntoEvent for PlayerToggleEntityAgeLockEvent {
    const EVENT_TYPE: EventType = EventType::PlayerToggleEntityAgeLockEvent;
    type Data = PlayerToggleEntityAgeLockEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerToggleEntityAgeLockEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerToggleEntityAgeLockEvent(data)
    }
}
