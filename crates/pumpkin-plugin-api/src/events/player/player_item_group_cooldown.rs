use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerItemGroupCooldownEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerItemGroupCooldownEvent`: a cooldown group is about to start. Change `cooldown` (ticks) or cancel.
pub struct PlayerItemGroupCooldownEvent;
impl FromIntoEvent for PlayerItemGroupCooldownEvent {
    const EVENT_TYPE: EventType = EventType::PlayerItemGroupCooldownEvent;
    type Data = PlayerItemGroupCooldownEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerItemGroupCooldownEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerItemGroupCooldownEvent(data)
    }
}
