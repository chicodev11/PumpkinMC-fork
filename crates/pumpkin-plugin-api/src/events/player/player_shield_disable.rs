use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerShieldDisableEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerShieldDisableEvent`: an attack disables the player's shield. Change `cooldown` or cancel.
pub struct PlayerShieldDisableEvent;
impl FromIntoEvent for PlayerShieldDisableEvent {
    const EVENT_TYPE: EventType = EventType::PlayerShieldDisableEvent;
    type Data = PlayerShieldDisableEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerShieldDisableEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerShieldDisableEvent(data)
    }
}
