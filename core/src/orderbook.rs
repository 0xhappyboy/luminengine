use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    thread::{self},
    time::Duration,
};

use crate::{matcher::Matcher, types::PushOrderEvent};

use serde::{Deserialize, Serialize};
use tokio::{join, task::JoinHandle};

/// order direction enumeration
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum OrderDirection {
    Buy,
    Sell,
}

/// order source channel enumeration
#[derive(Debug, Clone)]
pub enum OrderSourceChannel {
    Http,
    Tcp,
    Rcp,
}

// order book trade target abstract
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Target {
    price: f64,
}

impl Target {
    pub fn new(price: f64) -> Self {
        Self { price: price }
    }
}

/// for each order abstract
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Order {
    pub symbol: String,
    pub price: f64,
    pub order_direction: OrderDirection,
    pub ex: Option<String>,
}

impl Order {
    pub fn new(symbol: String, price: f64, order_direction: OrderDirection) -> Self {
        Self {
            symbol: symbol,
            price: price,
            order_direction: order_direction,
            ex: None,
        }
    }
}

/// order tree
#[derive(Debug, Clone)]
pub struct OrderTree {
    pub symbol: String,
    tree: HashMap<String, Vec<Order>>,
    // push buy order before event
    pub push_buy_order_before_event: Option<PushOrderEvent>,
    // push buy order after event
    pub push_buy_order_after_event: Option<PushOrderEvent>,
    // push sell order before event
    pub push_sell_order_before_event: Option<PushOrderEvent>,
    // push sell order after event
    pub push_sell_order_after_event: Option<PushOrderEvent>,
}

impl OrderTree {
    pub fn new(
        symbol: String,
        push_buy_order_before_event: Option<PushOrderEvent>,
        push_buy_order_after_event: Option<PushOrderEvent>,
        push_sell_order_before_event: Option<PushOrderEvent>,
        push_sell_order_after_event: Option<PushOrderEvent>,
    ) -> Self {
        Self {
            symbol: symbol,
            tree: HashMap::<String, Vec<Order>>::default(),
            push_buy_order_before_event: push_buy_order_before_event,
            push_buy_order_after_event: push_buy_order_after_event,
            push_sell_order_before_event: push_sell_order_before_event,
            push_sell_order_after_event: push_sell_order_after_event,
        }
    }
    // Determine whether the specified price exists in the order tree
    pub fn contains_price(&self, price: f64) -> bool {
        self.tree.contains_key(&price.to_string())
    }
    // Is the order tree empty?
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }
    // Push orders to the order tree
    pub fn push(&mut self, order: Order) {
        // before successfully placing the order
        match order.order_direction {
            OrderDirection::Buy => {
                // buy order
                if self.push_buy_order_before_event.is_some() {
                    self.push_buy_order_before_event.unwrap()(order.clone());
                }
            }
            OrderDirection::Sell => {
                // sell order
                if self.push_sell_order_before_event.is_some() {
                    self.push_sell_order_before_event.unwrap()(order.clone());
                }
            }
        }
        if self.contains_price(order.clone().price) {
            self.tree
                .get_mut(&order.clone().price.to_string())
                .unwrap()
                .push(order.clone());
        } else {
            self.tree
                .insert(order.clone().price.to_string(), vec![order.clone()]);
        }
        // after successfully placing the order
        match order.order_direction {
            OrderDirection::Buy => {
                if self.push_buy_order_after_event.is_some() {
                    self.push_buy_order_after_event.unwrap()(order);
                }
            }
            OrderDirection::Sell => {
                if self.push_sell_order_after_event.is_some() {
                    self.push_sell_order_after_event.unwrap()(order);
                }
            }
        }
    }
    /// specified price order quantity
    pub fn get_order_num_by_price(&self, price: f64) -> u64 {
        if self.tree.contains_key(&price.to_string()) {
            self.tree
                .get(&price.to_string())
                .unwrap()
                .len()
                .try_into()
                .unwrap()
        } else {
            0
        }
    }
    pub fn get_price_num(&self) -> u64 {
        self.tree.len().try_into().unwrap()
    }
    pub fn cancel(&mut self, order: Order) {}
}

#[derive(Debug, Clone)]
/// border book
pub struct OrderBook {
    pub symbol: String,
    pub bids: Arc<RwLock<OrderTree>>,
    pub asks: Arc<RwLock<OrderTree>>,
    pub matching: Arc<Mutex<bool>>,
}

impl OrderBook {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol: symbol.clone(),
            bids: Arc::new(RwLock::new(OrderTree::new(
                symbol.clone(),
                None,
                None,
                None,
                None,
            ))),
            asks: Arc::new(RwLock::new(OrderTree::new(
                symbol.clone(),
                None,
                None,
                None,
                None,
            ))),
            matching: Arc::new(Mutex::new(false)),
        }
    }
    // enble matcher engine
    pub fn enble_matcher(&self, matcher: Matcher) {
        let symbol = self.symbol.clone();
        let bids = Arc::clone(&self.bids);
        let asks = Arc::clone(&self.asks);
        tokio::spawn(matcher.match_order(symbol, bids, asks));
    }
    /// push order specific implementation logic.
    pub fn push_order(&mut self, order: Order) {
        match order.order_direction {
            OrderDirection::Buy => {
                let mut bids = self.bids.write().unwrap();
                bids.push(order);
            }
            OrderDirection::Sell => {
                let mut asks = self.asks.write().unwrap();
                asks.push(order);
            }
        }
    }
    /// matching order
    pub fn matching_order(&self) {}
    /// matching trading
    pub fn matching_trading(&self) {}
    /// get order num
    pub fn order_num(&self, order: Order) -> (u64, u64) {
        let (bids, asks) = self.get_order_copy();
        let bids = bids.read().unwrap();
        let asks = asks.read().unwrap();
        let bids_num = if bids.is_empty() {
            0
        } else {
            bids.get_order_num_by_price(order.price)
        };
        let asks_num = if asks.is_empty() {
            0
        } else {
            asks.get_order_num_by_price(order.price)
        };
        (bids_num.try_into().unwrap(), asks_num.try_into().unwrap())
    }
    /// order book storage
    pub fn storage() {}
    /// get order copy
    fn get_order_copy(&self) -> (Arc<RwLock<OrderTree>>, Arc<RwLock<OrderTree>>) {
        (Arc::clone(&self.bids), Arc::clone(&self.asks))
    }
    /// update push buy order before event
    pub fn update_push_buy_order_before_event(
        &mut self,
        push_buy_order_before_event: Option<PushOrderEvent>,
    ) -> Self {
        let mut bids = self.bids.write().unwrap();
        bids.push_buy_order_before_event = push_buy_order_before_event;
        self.clone()
    }
    /// update push buy order after event
    pub fn update_push_buy_order_after_event(
        &mut self,
        push_buy_order_after_event: Option<PushOrderEvent>,
    ) -> Self {
        let mut bids = self.bids.write().unwrap();
        bids.push_buy_order_after_event = push_buy_order_after_event;
        self.clone()
    }
    /// update push sell order before event
    pub fn update_push_sell_order_before_event(
        &mut self,
        push_sell_order_before_event: Option<PushOrderEvent>,
    ) -> Self {
        let mut asks = self.asks.write().unwrap();
        asks.push_sell_order_before_event = push_sell_order_before_event;
        self.clone()
    }
    /// update push sell order after event
    pub fn update_push_sell_order_after_event(
        &mut self,
        push_sell_order_after_event: Option<PushOrderEvent>,
    ) -> Self {
        let mut asks = self.asks.write().unwrap();
        asks.push_sell_order_after_event = push_sell_order_after_event;
        self.clone()
    }
}
