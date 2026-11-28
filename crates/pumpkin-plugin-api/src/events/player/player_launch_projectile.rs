use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerLaunchProjectileEventData};

use super::super::FromIntoEvent;

/// Paper `PlayerLaunchProjectileEvent`: a player throws a projectile item (snowball, egg, ender pearl,
pub struct PlayerLaunchProjectileEvent;
impl FromIntoEvent for PlayerLaunchProjectileEvent {
    const EVENT_TYPE: EventType = EventType::PlayerLaunchProjectileEvent;
    type Data = PlayerLaunchProjectileEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerLaunchProjectileEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerLaunchProjectileEvent(data)
    }
}
