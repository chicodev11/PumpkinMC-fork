use crate::wit::pumpkin::plugin::event::{Event, EventType, WitchThrowPotionEventData};

use super::super::FromIntoEvent;

/// A witch throws a potion (Paper `WitchThrowPotionEvent`).
pub struct WitchThrowPotionEvent;
impl FromIntoEvent for WitchThrowPotionEvent {
    const EVENT_TYPE: EventType = EventType::WitchThrowPotionEvent;
    type Data = WitchThrowPotionEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::WitchThrowPotionEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::WitchThrowPotionEvent(data)
    }
}
