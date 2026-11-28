use crate::wit::pumpkin::plugin::event::{Event, EventType, SkeletonHorseTrapEventData};

use super::super::FromIntoEvent;

/// Paper `SkeletonHorseTrapEvent`: a skeleton trap horse is set off by nearby players.
pub struct SkeletonHorseTrapEvent;
impl FromIntoEvent for SkeletonHorseTrapEvent {
    const EVENT_TYPE: EventType = EventType::SkeletonHorseTrapEvent;
    type Data = SkeletonHorseTrapEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::SkeletonHorseTrapEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::SkeletonHorseTrapEvent(data)
    }
}
