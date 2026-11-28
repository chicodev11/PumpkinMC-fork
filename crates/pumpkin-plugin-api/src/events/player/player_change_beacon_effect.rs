use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerChangeBeaconEffectEventData};

use super::super::FromIntoEvent;

/// A player changes a beacon's effects (Paper `PlayerChangeBeaconEffectEvent`).
pub struct PlayerChangeBeaconEffectEvent;
impl FromIntoEvent for PlayerChangeBeaconEffectEvent {
    const EVENT_TYPE: EventType = EventType::PlayerChangeBeaconEffectEvent;
    type Data = PlayerChangeBeaconEffectEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerChangeBeaconEffectEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerChangeBeaconEffectEvent(data)
    }
}
