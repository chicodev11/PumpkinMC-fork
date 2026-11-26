use crate::wit::pumpkin::plugin::event::{ClientItemViewEventData, Event, EventType};

use super::super::FromIntoEvent;

/// An item stack written into a packet for one player: replace `item` to change only what that
/// client shows (client-side names, lore, `item_model`). Fired for every stack in every packet
/// while a handler is registered, so keep handlers quick.
pub struct ClientItemViewEvent;

impl FromIntoEvent for ClientItemViewEvent {
    const EVENT_TYPE: EventType = EventType::ClientItemViewEvent;
    type Data = ClientItemViewEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::ClientItemViewEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::ClientItemViewEvent(data)
    }
}
