use crate::wit::pumpkin::plugin::event::{ElderGuardianAppearanceEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `ElderGuardianAppearanceEvent`: an elder guardian curses a player (the face and mining fatigue).
pub struct ElderGuardianAppearanceEvent;
impl FromIntoEvent for ElderGuardianAppearanceEvent {
    const EVENT_TYPE: EventType = EventType::ElderGuardianAppearanceEvent;
    type Data = ElderGuardianAppearanceEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::ElderGuardianAppearanceEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::ElderGuardianAppearanceEvent(data)
    }
}
