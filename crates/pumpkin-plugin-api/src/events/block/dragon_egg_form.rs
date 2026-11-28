use crate::wit::pumpkin::plugin::event::{DragonEggFormEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `DragonEggFormEvent`: the dragon egg appears after the first dragon kill.
pub struct DragonEggFormEvent;
impl FromIntoEvent for DragonEggFormEvent {
    const EVENT_TYPE: EventType = EventType::DragonEggFormEvent;
    type Data = DragonEggFormEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::DragonEggFormEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::DragonEggFormEvent(data)
    }
}
