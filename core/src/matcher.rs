/// transaction matching engine
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
    thread::{self, spawn},
    time::Duration,
};

use crate::orderbook::{self, OrderBook, OrderTree};

pub trait Matcher {
    fn match_order(
        &self,
        orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
}

/// match engine
#[derive(Debug, Clone)]
pub struct MatchEngine {}

impl MatchEngine {
    pub fn new() -> Self {
        Self {}
    }
}

impl Matcher for MatchEngine {
    fn match_order(
        &self,
        orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            loop {
                for orderbook in orderbooks.read().unwrap().values() {
                    if !*orderbook.read().unwrap().matching.lock().unwrap() {
                        let bids = &orderbook.read().unwrap().bids;
                        let asks = &orderbook.read().unwrap().asks;
                        // Operations on the order tree.
                        thread::sleep(Duration::from_millis(1500));
                    }
                }
            }
        })
    }
}

/// match record
pub struct MatchRecord {}
impl MatchRecord {}
