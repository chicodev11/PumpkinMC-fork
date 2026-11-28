use crate::wit::pumpkin::plugin::event::{Event, EventType, WitchReadyPotionEventData};

use super::super::FromIntoEvent;

/// A witch readies a potion (Paper `WitchReadyPotionEvent`).
pub struct WitchReadyPotionEvent;
impl FromIntoEvent for WitchReadyPotionEvent {
    const EVENT_TYPE: EventType = EventType::WitchReadyPotionEvent;
    type Data = WitchReadyPotionEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::WitchReadyPotionEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::WitchReadyPotionEvent(data)
    }
}
