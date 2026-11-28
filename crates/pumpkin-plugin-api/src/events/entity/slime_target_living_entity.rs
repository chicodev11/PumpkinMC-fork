use crate::wit::pumpkin::plugin::event::{Event, EventType, SlimeTargetLivingEntityEventData};

use super::super::FromIntoEvent;

/// A slime targets an entity (Paper `SlimeTargetLivingEntityEvent`).
pub struct SlimeTargetLivingEntityEvent;
impl FromIntoEvent for SlimeTargetLivingEntityEvent {
    const EVENT_TYPE: EventType = EventType::SlimeTargetLivingEntityEvent;
    type Data = SlimeTargetLivingEntityEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::SlimeTargetLivingEntityEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::SlimeTargetLivingEntityEvent(data)
    }
}
