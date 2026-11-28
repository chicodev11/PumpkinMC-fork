use crate::wit::pumpkin::plugin::event::{EntityPushedByEntityAttackEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An entity is knocked back by an attack (Paper `EntityPushedByEntityAttackEvent`).
pub struct EntityPushedByEntityAttackEvent;
impl FromIntoEvent for EntityPushedByEntityAttackEvent {
    const EVENT_TYPE: EventType = EventType::EntityPushedByEntityAttackEvent;
    type Data = EntityPushedByEntityAttackEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityPushedByEntityAttackEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityPushedByEntityAttackEvent(data)
    }
}
