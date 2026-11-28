use crate::wit::pumpkin::plugin::event::{BeaconEffectEventData, Event, EventType};

use super::super::FromIntoEvent;

/// Paper `BeaconEffectEvent`: a beacon gives a player its effect. Change the effect or cancel.
pub struct BeaconEffectEvent;
impl FromIntoEvent for BeaconEffectEvent {
    const EVENT_TYPE: EventType = EventType::BeaconEffectEvent;
    type Data = BeaconEffectEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::BeaconEffectEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::BeaconEffectEvent(data)
    }
}
