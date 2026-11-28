use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerSetSpawnEventData};

use super::super::FromIntoEvent;

/// A player's respawn point is set (Paper `PlayerSetSpawnEvent`).
pub struct PlayerSetSpawnEvent;
impl FromIntoEvent for PlayerSetSpawnEvent {
    const EVENT_TYPE: EventType = EventType::PlayerSetSpawnEvent;
    type Data = PlayerSetSpawnEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerSetSpawnEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerSetSpawnEvent(data)
    }
}
