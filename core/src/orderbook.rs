use crate::{
    matchengine::{matcher::Matcher, slfe::Slfe, MatchEngineConfig}, order::Order, price::Price, target::Target
};
use chrono::Utc;
use dashmap::DashMap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, RwLock},
    time::Instant,
};

lazy_static! {
    static ref ORDER_BOOKS: DashMap<Target, Arc<RwLock<OrderBook>>> = DashMap::new();
}

/// order source channel enumeration
#[derive(Debug, Clone)]
pub enum OrderSourceChannel {
    Http,
    Tcp,
    Rcp,
}

/// Orderbook collection type.
#[derive(Debug, Clone, Default)]
pub struct OrderBooks;

impl OrderBooks {
    pub fn contains_symbol(symbol: String) -> bool {
        // self.orderbooks.contains_key(&symbol)
        ORDER_BOOKS.contains_key(&Target::new(symbol))
    }
    pub fn get_orderbook_by_symbol(symbol: String) -> Option<Arc<RwLock<OrderBook>>> {
        if OrderBooks::contains_symbol(symbol.clone()) {
            Some(Arc::clone(
                &ORDER_BOOKS.get(&Target { symbol: symbol }).unwrap(),
            ))
        } else {
            None
        }
    }
    pub fn order_num() -> u64 {
        ORDER_BOOKS.len().try_into().unwrap()
    }
    pub fn insert(symbol: String, orderbook: Arc<RwLock<OrderBook>>) -> Result<String, String> {
        if !OrderBooks::contains_symbol(symbol.clone()) {
            ORDER_BOOKS.insert(
                Target {
                    symbol: symbol.clone(),
                },
                orderbook,
            );
            ORDER_BOOKS
                .get(&Target {
                    symbol: symbol.clone(),
                })
                .unwrap()
                .read()
                .unwrap()
                .enble_matcher(Matcher::new());
            Ok("new order book added successfully".to_string())
        } else {
            Err("symbol already exists".to_string())
        }
    }
}

#[derive(Debug, Clone)]
/// border book
pub struct OrderBook {
    pub target: Arc<RwLock<Target>>,
    pub match_engine: Slfe,
}

impl OrderBook {
    pub fn new(target: Target) -> Self {
        Self {
            target: Arc::new(RwLock::new(target.clone())),
            match_engine: Slfe::new(Some(MatchEngineConfig::default())),
        }
    }
    // enble matcher engine
    pub fn enble_matcher(&self, matcher: Matcher) {
        let symbol = self.target.read().unwrap().symbol.clone();
        tokio::spawn(matcher.match_order(symbol));
    }
    /// push order specific implementation logic.
    pub fn push_order(&mut self, order: Order) {
        self.match_engine.add_order(order);
    }
    /// matching order
    pub fn matching_order(&self) {}
    /// matching trading
    pub fn matching_trading(&self) {}
    /// order book storage
    pub fn storage() {}
}
