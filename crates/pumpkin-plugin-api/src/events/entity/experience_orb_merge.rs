use crate::wit::pumpkin::plugin::event::{Event, EventType, ExperienceOrbMergeEventData};

use super::super::FromIntoEvent;

/// Paper `ExperienceOrbMergeEvent`: one experience orb absorbs another.
pub struct ExperienceOrbMergeEvent;
impl FromIntoEvent for ExperienceOrbMergeEvent {
    const EVENT_TYPE: EventType = EventType::ExperienceOrbMergeEvent;
    type Data = ExperienceOrbMergeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::ExperienceOrbMergeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::ExperienceOrbMergeEvent(data)
    }
}
