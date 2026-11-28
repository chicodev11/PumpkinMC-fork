use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerDeepSleepEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerDeepSleepEvent`: the player has slept long enough to count for skipping the night.
pub struct PlayerDeepSleepEvent;
impl FromIntoEvent for PlayerDeepSleepEvent {
    const EVENT_TYPE: EventType = EventType::PlayerDeepSleepEvent;
    type Data = PlayerDeepSleepEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerDeepSleepEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerDeepSleepEvent(data)
    }
}
