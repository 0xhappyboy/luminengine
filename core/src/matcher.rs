/// transaction matching engine
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
    thread::{self, spawn},
    time::Duration,
};

use crate::orderbook::{self, OrderBook, OrderTree};

#[derive(Debug, Clone)]
pub struct Matcher;
impl Matcher {
    pub fn new() -> Self {
        Self {}
    }
    pub fn match_order(
        &self,
        symbol: String,
        bids: Arc<RwLock<OrderTree>>,
        asks: Arc<RwLock<OrderTree>>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move { loop {} })
    }
}
