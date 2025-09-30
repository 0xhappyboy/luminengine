use std::{
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use crate::orderbook::OrderTree;

pub trait Matcher {
    fn match_order(
        &self,
        buy_orders: Arc<RwLock<OrderTree>>,
        sell_orders: Arc<RwLock<OrderTree>>,
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
        bids: Arc<RwLock<OrderTree>>,
        asks: Arc<RwLock<OrderTree>>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            loop {
                let bids = bids.read().unwrap();
                let asks = asks.read().unwrap();
                // Operations on the order tree.
                println!(
                    "order num by price :{:?} {:?}",
                    bids.get_order_num_by_price(11.1),
                    asks.get_order_num_by_price(11.1)
                );
                thread::sleep(Duration::from_millis(1500));
            }
        })
    }
}

/// match record
pub struct MatchRecord {}
impl MatchRecord {}
