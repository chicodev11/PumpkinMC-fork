use crate::wit::pumpkin::plugin::event::{AnvilDamagedEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `AnvilDamagedEvent`: using an anvil damages it. `damage-state` is FULL, CHIPPED, DAMAGED or BROKEN.
pub struct AnvilDamagedEvent;
impl FromIntoEvent for AnvilDamagedEvent {
    const EVENT_TYPE: EventType = EventType::AnvilDamagedEvent;
    type Data = AnvilDamagedEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::AnvilDamagedEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::AnvilDamagedEvent(data)
    }
}
