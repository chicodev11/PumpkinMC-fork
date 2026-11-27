use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerPickupExperienceEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerPickupExperienceEvent`: a player is about to pick up an experience orb.
pub struct PlayerPickupExperienceEvent;
impl FromIntoEvent for PlayerPickupExperienceEvent {
    const EVENT_TYPE: EventType = EventType::PlayerPickupExperienceEvent;
    type Data = PlayerPickupExperienceEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerPickupExperienceEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerPickupExperienceEvent(data)
    }
}
