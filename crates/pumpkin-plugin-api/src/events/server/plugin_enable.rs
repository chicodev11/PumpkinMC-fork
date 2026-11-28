use crate::wit::pumpkin::plugin::event::{Event, EventType, PluginEnableEventData};

use super::super::FromIntoEvent;

/// A plugin was enabled (Paper `PluginEnableEvent`).
pub struct PluginEnableEvent;
impl FromIntoEvent for PluginEnableEvent {
    const EVENT_TYPE: EventType = EventType::PluginEnableEvent;
    type Data = PluginEnableEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PluginEnableEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PluginEnableEvent(data)
    }
}
