use crate::orderbook::Order;

// push order event
pub type PushOrderEvent = fn(order: Order);
