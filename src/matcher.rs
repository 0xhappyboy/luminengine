use std::sync::{Arc, Mutex};

use crate::orderbook::OrderTree;

pub trait Matcher {
    fn match_order(buy_orders: Arc<Mutex<OrderTree>>, sell_orders: Arc<Mutex<OrderTree>>);
}

/// match engine
#[derive(Debug, Clone)]
pub struct MatchEngine {}

impl Matcher for MatchEngine {
    fn match_order(bids: Arc<Mutex<OrderTree>>, asks: Arc<Mutex<OrderTree>>) {
        todo!()
    }
}

impl MatchEngine {
    pub fn new() -> Self {
        Self {}
    }
}

/// match record
pub struct MatchRecord {}
impl MatchRecord {}
