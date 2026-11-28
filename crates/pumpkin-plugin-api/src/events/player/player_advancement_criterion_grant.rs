use crate::wit::pumpkin::plugin::event::{
    Event, EventType, PlayerAdvancementCriterionGrantEventData,
};

use super::super::FromIntoEvent;

/// A player is granted an advancement criterion (Paper `PlayerAdvancementCriterionGrantEvent`).
pub struct PlayerAdvancementCriterionGrantEvent;
impl FromIntoEvent for PlayerAdvancementCriterionGrantEvent {
    const EVENT_TYPE: EventType = EventType::PlayerAdvancementCriterionGrantEvent;
    type Data = PlayerAdvancementCriterionGrantEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerAdvancementCriterionGrantEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerAdvancementCriterionGrantEvent(data)
    }
}
