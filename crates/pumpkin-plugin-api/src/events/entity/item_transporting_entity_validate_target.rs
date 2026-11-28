use crate::wit::pumpkin::plugin::event::{
    Event, EventType, ItemTransportingEntityValidateTargetEventData,
};

use super::super::FromIntoEvent;

/// A copper golem checks whether a container is a valid target (Paper `ItemTransportingEntityValidateTargetEvent`).
pub struct ItemTransportingEntityValidateTargetEvent;
impl FromIntoEvent for ItemTransportingEntityValidateTargetEvent {
    const EVENT_TYPE: EventType = EventType::ItemTransportingEntityValidateTargetEvent;
    type Data = ItemTransportingEntityValidateTargetEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::ItemTransportingEntityValidateTargetEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::ItemTransportingEntityValidateTargetEvent(data)
    }
}
