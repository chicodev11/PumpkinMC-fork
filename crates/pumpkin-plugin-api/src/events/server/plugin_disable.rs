use crate::wit::pumpkin::plugin::event::{Event, EventType, PluginDisableEventData};

use super::super::FromIntoEvent;

/// A plugin is being disabled (Paper `PluginDisableEvent`).
pub struct PluginDisableEvent;
impl FromIntoEvent for PluginDisableEvent {
    const EVENT_TYPE: EventType = EventType::PluginDisableEvent;
    type Data = PluginDisableEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PluginDisableEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PluginDisableEvent(data)
    }
}
