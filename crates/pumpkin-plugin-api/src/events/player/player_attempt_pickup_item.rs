use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerAttemptPickupItemEventData};

use super::super::FromIntoEvent;

/// A player touches an item they could pick up (Paper `PlayerAttemptPickupItemEvent`).
pub struct PlayerAttemptPickupItemEvent;
impl FromIntoEvent for PlayerAttemptPickupItemEvent {
    const EVENT_TYPE: EventType = EventType::PlayerAttemptPickupItemEvent;
    type Data = PlayerAttemptPickupItemEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerAttemptPickupItemEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerAttemptPickupItemEvent(data)
    }
}
