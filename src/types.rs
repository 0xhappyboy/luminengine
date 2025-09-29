use crate::orderbook::Order;

// push order event
type PushOrderEvent = fn(order: Order);
