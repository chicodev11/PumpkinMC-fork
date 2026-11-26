use super::super::FromIntoEvent;
use crate::wit::pumpkin::plugin::event::{Event, EventType, PlayerTradeEventData};

/// Event triggered when a player takes the result of a trade (Paper `PlayerPurchaseEvent`);
/// cancelling it stops the trade.
pub struct PlayerTradeEvent;
impl FromIntoEvent for PlayerTradeEvent {
    const EVENT_TYPE: EventType = EventType::PlayerTradeEvent;
    type Data = PlayerTradeEventData;

    fn data_from_event(event: Event) -> Self::Data {
        match event {
            Event::PlayerTradeEvent(data) => data,
            _ => panic!("unexpected event"),
        }
    }

    fn data_into_event(data: Self::Data) -> Event {
        Event::PlayerTradeEvent(data)
    }
}
