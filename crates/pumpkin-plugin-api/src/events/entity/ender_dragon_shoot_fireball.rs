use crate::wit::pumpkin::plugin::event::{EnderDragonShootFireballEventData, Event, EventType};

use super::super::FromIntoEvent;

/// The ender dragon shoots a fireball (Paper `EnderDragonShootFireballEvent`).
pub struct EnderDragonShootFireballEvent;
impl FromIntoEvent for EnderDragonShootFireballEvent {
    const EVENT_TYPE: EventType = EventType::EnderDragonShootFireballEvent;
    type Data = EnderDragonShootFireballEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::EnderDragonShootFireballEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::EnderDragonShootFireballEvent(data)
    }
}
