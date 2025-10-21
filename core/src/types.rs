use std::sync::{Arc, RwLock};

use crate::orderbook::{Order, OrderBooks};
use lazy_static::lazy_static;

/// push order event
pub type PushOrderEvent = fn(order: Order);

/// matcher processing process, is used to rewrite the logic of matching orders in the order book.
pub type MatcherProcessingProcess = fn();
