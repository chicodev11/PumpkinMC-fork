use crate::wit::pumpkin::plugin::event::{EntityEffectTickEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An effect ticks on an entity (Paper `EntityEffectTickEvent`).
pub struct EntityEffectTickEvent;
impl FromIntoEvent for EntityEffectTickEvent {
    const EVENT_TYPE: EventType = EventType::EntityEffectTickEvent;
    type Data = EntityEffectTickEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityEffectTickEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityEffectTickEvent(data)
    }
}
