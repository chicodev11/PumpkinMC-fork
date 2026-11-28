use crate::wit::pumpkin::plugin::event::{EndermanAttackPlayerEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `EndermanAttackPlayerEvent`: an enderman checks whether a player stares at it. Change `should-attack`.
pub struct EndermanAttackPlayerEvent;
impl FromIntoEvent for EndermanAttackPlayerEvent {
    const EVENT_TYPE: EventType = EventType::EndermanAttackPlayerEvent;
    type Data = EndermanAttackPlayerEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EndermanAttackPlayerEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EndermanAttackPlayerEvent(data)
    }
}
