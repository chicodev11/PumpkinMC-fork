use crate::wit::pumpkin::plugin::event::{EntityEquipmentChangedEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An entity's equipment changed (Paper `EntityEquipmentChangedEvent`).
pub struct EntityEquipmentChangedEvent;
impl FromIntoEvent for EntityEquipmentChangedEvent {
    const EVENT_TYPE: EventType = EventType::EntityEquipmentChangedEvent;
    type Data = EntityEquipmentChangedEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityEquipmentChangedEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityEquipmentChangedEvent(data)
    }
}
