use crate::wit::pumpkin::plugin::event::{Event, EventType, WitchConsumePotionEventData};

use super::super::FromIntoEvent;

/// A witch drinks a potion (Paper `WitchConsumePotionEvent`).
pub struct WitchConsumePotionEvent;
impl FromIntoEvent for WitchConsumePotionEvent {
    const EVENT_TYPE: EventType = EventType::WitchConsumePotionEvent;
    type Data = WitchConsumePotionEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::WitchConsumePotionEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::WitchConsumePotionEvent(data)
    }
}
