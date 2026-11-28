use crate::wit::pumpkin::plugin::event::{EntityAttemptSpinAttackEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An entity in a riptide spin hits another (Paper `EntityAttemptSpinAttackEvent`).
pub struct EntityAttemptSpinAttackEvent;
impl FromIntoEvent for EntityAttemptSpinAttackEvent {
    const EVENT_TYPE: EventType = EventType::EntityAttemptSpinAttackEvent;
    type Data = EntityAttemptSpinAttackEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EntityAttemptSpinAttackEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EntityAttemptSpinAttackEvent(data)
    }
}
