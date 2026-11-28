use crate::wit::pumpkin::plugin::event::{Event, EventType, PreCreatureSpawnEventData};

use super::super::FromIntoEvent;

/// A mob is about to spawn naturally (Paper `PreCreatureSpawnEvent`).
pub struct PreCreatureSpawnEvent;
impl FromIntoEvent for PreCreatureSpawnEvent {
    const EVENT_TYPE: EventType = EventType::PreCreatureSpawnEvent;
    type Data = PreCreatureSpawnEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PreCreatureSpawnEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PreCreatureSpawnEvent(data)
    }
}
