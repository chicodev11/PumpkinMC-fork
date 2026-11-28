use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerItemCooldownEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerItemCooldownEvent`: an item's use starts a cooldown. Change `cooldown` (ticks) or cancel.
pub struct PlayerItemCooldownEvent;
impl FromIntoEvent for PlayerItemCooldownEvent {
    const EVENT_TYPE: EventType = EventType::PlayerItemCooldownEvent;
    type Data = PlayerItemCooldownEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerItemCooldownEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerItemCooldownEvent(data)
    }
}
