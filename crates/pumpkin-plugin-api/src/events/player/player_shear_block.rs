use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerShearBlockEventData};

use super::super::FromIntoEvent;

/// A player shears a block such as a pumpkin or a beehive (Paper `PlayerShearBlockEvent`).
pub struct PlayerShearBlockEvent;
impl FromIntoEvent for PlayerShearBlockEvent {
    const EVENT_TYPE: EventType = EventType::PlayerShearBlockEvent;
    type Data = PlayerShearBlockEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerShearBlockEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerShearBlockEvent(data)
    }
}
