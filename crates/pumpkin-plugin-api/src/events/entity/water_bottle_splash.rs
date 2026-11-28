use crate::wit::pumpkin::plugin::event::{Event, EventType, WaterBottleSplashEventData};

use super::super::FromIntoEvent;

/// A splash water bottle lands (Paper `WaterBottleSplashEvent`).
pub struct WaterBottleSplashEvent;
impl FromIntoEvent for WaterBottleSplashEvent {
    const EVENT_TYPE: EventType = EventType::WaterBottleSplashEvent;
    type Data = WaterBottleSplashEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::WaterBottleSplashEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::WaterBottleSplashEvent(data)
    }
}
