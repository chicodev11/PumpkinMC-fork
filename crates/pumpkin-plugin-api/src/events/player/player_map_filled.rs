use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerMapFilledEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerMapFilledEvent`: an empty map was used. Change `created-map` to give another item.
pub struct PlayerMapFilledEvent;
impl FromIntoEvent for PlayerMapFilledEvent {
    const EVENT_TYPE: EventType = EventType::PlayerMapFilledEvent;
    type Data = PlayerMapFilledEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerMapFilledEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerMapFilledEvent(data)
    }
}
