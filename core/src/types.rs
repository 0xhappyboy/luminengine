use crate::orderbook::Order;

/// push order event
pub type PushOrderEvent = fn(order: Order);

/// matcher processing process, is used to rewrite the logic of matching orders in the order book.
pub type MatcherProcessingProcess = fn();
