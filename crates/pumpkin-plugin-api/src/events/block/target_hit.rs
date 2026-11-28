use crate::wit::pumpkin::plugin::event::{Event, EventType, TargetHitEventData};

use super::super::FromIntoEvent;

/// Paper `TargetHitEvent`: a projectile hit a target block. Change `signal-strength` (0-15) or cancel.
pub struct TargetHitEvent;
impl FromIntoEvent for TargetHitEvent {
    const EVENT_TYPE: EventType = EventType::TargetHitEvent;
    type Data = TargetHitEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::TargetHitEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::TargetHitEvent(data)
    }
}
