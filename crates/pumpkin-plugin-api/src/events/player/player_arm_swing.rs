use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerArmSwingEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerArmSwingEvent`: the player swings a hand (`HAND` or `OFF_HAND`).
pub struct PlayerArmSwingEvent;
impl FromIntoEvent for PlayerArmSwingEvent {
    const EVENT_TYPE: EventType = EventType::PlayerArmSwingEvent;
    type Data = PlayerArmSwingEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerArmSwingEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerArmSwingEvent(data)
    }
}
