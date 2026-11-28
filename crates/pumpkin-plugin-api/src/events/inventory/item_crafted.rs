use crate::wit::pumpkin::plugin::event::{Event, EventType, ItemCraftedEventData};

use super::super::FromIntoEvent;

/// A player crafted an item (Paper `ItemCraftedEvent`).
pub struct ItemCraftedEvent;
impl FromIntoEvent for ItemCraftedEvent {
    const EVENT_TYPE: EventType = EventType::ItemCraftedEvent;
    type Data = ItemCraftedEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::ItemCraftedEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::ItemCraftedEvent(data)
    }
}
