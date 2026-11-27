use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerTrackEntityEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerTrackEntityEvent`: an entity is about to be shown to a player. Cancel to skip it.
pub struct PlayerTrackEntityEvent;
impl FromIntoEvent for PlayerTrackEntityEvent {
    const EVENT_TYPE: EventType = EventType::PlayerTrackEntityEvent;
    type Data = PlayerTrackEntityEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerTrackEntityEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerTrackEntityEvent(data)
    }
}
