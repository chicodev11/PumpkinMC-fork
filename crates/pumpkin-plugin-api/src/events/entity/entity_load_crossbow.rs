use crate::wit::pumpkin::plugin::event::{EntityLoadCrossbowEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `EntityLoadCrossbowEvent`: a crossbow is loaded. Set `consume-item` to false to keep the ammo.
pub struct EntityLoadCrossbowEvent;
impl FromIntoEvent for EntityLoadCrossbowEvent {
    const EVENT_TYPE: EventType = EventType::EntityLoadCrossbowEvent;
    type Data = EntityLoadCrossbowEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityLoadCrossbowEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityLoadCrossbowEvent(data)
    }
}
