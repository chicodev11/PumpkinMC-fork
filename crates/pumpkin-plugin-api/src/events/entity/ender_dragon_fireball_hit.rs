use crate::wit::pumpkin::plugin::event::{EnderDragonFireballHitEventData, Event, EventType};

use super::super::FromIntoEvent;

/// A dragon fireball lands (Paper `EnderDragonFireballHitEvent`).
pub struct EnderDragonFireballHitEvent;
impl FromIntoEvent for EnderDragonFireballHitEvent {
    const EVENT_TYPE: EventType = EventType::EnderDragonFireballHitEvent;
    type Data = EnderDragonFireballHitEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EnderDragonFireballHitEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EnderDragonFireballHitEvent(data)
    }
}
