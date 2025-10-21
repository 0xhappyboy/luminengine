use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use crate::{
    matcher::Matcher,
    price::{AskPrice, BidPrice, Price},
    target::Target,
    types::PushOrderEvent,
};

use chrono::Utc;
use dashmap::DashMap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

lazy_static! {
    static ref ORDER_BOOKS: DashMap<Target, Arc<RwLock<OrderBook>>> = DashMap::new();
}

/// order direction enumeration
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum OrderDirection {
    Buy,
    Sell,
    None,
}

impl OrderDirection {
    pub fn from_string(s: String) -> OrderDirection {
        if s == "Buy".to_string() {
            OrderDirection::Buy
        } else if s == "Sell".to_string() {
            OrderDirection::Sell
        } else {
            OrderDirection::None
        }
    }
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
    pub bids: Arc<RwLock<OrderTree<BidPrice>>>,
    pub asks: Arc<RwLock<OrderTree<AskPrice>>>,
}

impl OrderBook {
    pub fn new(target: Target) -> Self {
        Self {
            target: Arc::new(RwLock::new(target.clone())),
            bids: Arc::new(RwLock::new(OrderTree::<BidPrice>::new(
                target.symbol.clone(),
                None,
                None,
                None,
                None,
            ))),
            asks: Arc::new(RwLock::new(OrderTree::<AskPrice>::new(
                target.symbol.clone(),
                None,
                None,
                None,
                None,
            ))),
        }
    }
    // enble matcher engine
    pub fn enble_matcher(&self, matcher: Matcher) {
        let symbol = self.target.read().unwrap().symbol.clone();
        let bids = Arc::clone(&self.bids);
        let asks = Arc::clone(&self.asks);
        tokio::spawn(matcher.match_order(symbol, bids, asks));
    }
    /// push order specific implementation logic.
    pub fn push_order(&mut self, order: Order) {
        match order.direction {
            OrderDirection::Buy => {
                let mut bids = self.bids.write().unwrap();
                bids.push(order);
            }
            OrderDirection::Sell => {
                let mut asks = self.asks.write().unwrap();
                asks.push(order);
            }
            OrderDirection::None => (),
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
    fn get_order_copy(
        &self,
    ) -> (
        Arc<RwLock<OrderTree<BidPrice>>>,
        Arc<RwLock<OrderTree<AskPrice>>>,
    ) {
        (Arc::clone(&self.bids), Arc::clone(&self.asks))
    }
    /// update push buy order before event
    pub fn update_push_buy_order_before_event(
        &mut self,
        push_buy_order_before_event: Option<PushOrderEvent>,
    ) {
        let mut bids = self.bids.write().unwrap();
        bids.push_buy_order_before_event = push_buy_order_before_event;
    }
    /// update push buy order after event
    pub fn update_push_buy_order_after_event(
        &mut self,
        push_buy_order_after_event: Option<PushOrderEvent>,
    ) {
        let mut bids = self.bids.write().unwrap();
        bids.push_buy_order_after_event = push_buy_order_after_event;
    }
    /// update push sell order before event
    pub fn update_push_sell_order_before_event(
        &mut self,
        push_sell_order_before_event: Option<PushOrderEvent>,
    ) {
        let mut asks = self.asks.write().unwrap();
        asks.push_sell_order_before_event = push_sell_order_before_event;
    }
    /// update push sell order after event
    pub fn update_push_sell_order_after_event(
        &mut self,
        push_sell_order_after_event: Option<PushOrderEvent>,
    ) {
        let mut asks = self.asks.write().unwrap();
        asks.push_sell_order_after_event = push_sell_order_after_event;
    }
}

/// order tree
#[derive(Debug, Clone)]
pub struct OrderTree<P>
where
    P: Price + Ord + PartialOrd + PartialEq + Eq,
{
    pub symbol: String,
    // k: price, v: order list
    // tree: HashMap<String, Vec<Order>>,
    tree: BTreeMap<P, Vec<Order>>,
    // push buy order before event
    pub push_buy_order_before_event: Option<PushOrderEvent>,
    // push buy order after event
    pub push_buy_order_after_event: Option<PushOrderEvent>,
    // push sell order before event
    pub push_sell_order_before_event: Option<PushOrderEvent>,
    // push sell order after event
    pub push_sell_order_after_event: Option<PushOrderEvent>,
}

impl<P> OrderTree<P>
where
    P: Price + Ord + PartialOrd + PartialEq + Eq,
{
    pub fn new(
        symbol: String,
        push_buy_order_before_event: Option<PushOrderEvent>,
        push_buy_order_after_event: Option<PushOrderEvent>,
        push_sell_order_before_event: Option<PushOrderEvent>,
        push_sell_order_after_event: Option<PushOrderEvent>,
    ) -> Self {
        Self {
            symbol: symbol,
            tree: BTreeMap::<P, Vec<Order>>::new(),
            push_buy_order_before_event: push_buy_order_before_event,
            push_buy_order_after_event: push_buy_order_after_event,
            push_sell_order_before_event: push_sell_order_before_event,
            push_sell_order_after_event: push_sell_order_after_event,
        }
    }
    // Determine whether the specified price exists in the order tree
    pub fn contains_price(&self, price: f64) -> bool {
        self.tree.contains_key(&P::new(price))
    }
    // Is the order tree empty?
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }
    // Push orders to the order tree
    pub fn push(&mut self, order: Order) {
        // before successfully placing the order
        match order.direction {
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
            OrderDirection::None => (),
        }
        if self.contains_price(order.clone().price) {
            self.tree
                .get_mut(&P::new(order.price))
                .unwrap()
                .push(order.clone());
        } else {
            self.tree.insert(P::new(order.price), vec![order.clone()]);
        }
        // after successfully placing the order
        match order.direction {
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
            OrderDirection::None => (),
        }
    }
    /// specified price order quantity
    pub fn get_order_num_by_price(&self, price: f64) -> u64 {
        if self.contains_price(price) {
            self.tree
                .get(&P::new(price))
                .unwrap()
                .len()
                .try_into()
                .unwrap()
        } else {
            0
        }
    }
    /// get order list by price
    pub fn get_order_vec_by_price(&self, price: f64) -> Vec<Order> {
        if self.contains_price(price) {
            self.tree.get(&P::new(price)).unwrap().to_vec()
        } else {
            vec![]
        }
    }
    pub fn get_price_num(&self) -> u64 {
        self.tree.len().try_into().unwrap()
    }
    /// get order tree
    pub fn order_tree(&self) -> &BTreeMap<P, Vec<Order>> {
        &self.tree
    }
    pub fn cancel(&mut self, order: Order) {}
}

/// for each order abstract
///
/// # Field
/// * id - order id
/// * symbol - symbol
/// * price - price
/// * direction - order direction
/// * quantity - quote unit overall quantity
/// * remaining - quote unit remaining quantity
/// * filled - quote unit completed quantity
/// * crt_time - order create time
/// * ex - Extend fields
///
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub price: f64,
    pub direction: OrderDirection,
    pub quantity: f64,
    pub remaining: f64,
    pub filled: f64,
    pub crt_time: String,
    pub ex: Option<String>,
}

impl Order {
    pub fn new(
        id: String,
        symbol: String,
        price: f64,
        quantity: f64,
        order_direction: OrderDirection,
    ) -> Self {
        Self {
            id: id,
            symbol: symbol,
            price: price,
            direction: order_direction,
            quantity: quantity,
            remaining: quantity,
            filled: 0.0,
            crt_time: Utc::now().to_string(),
            ex: None,
        }
    }
    pub fn from_rpc_order(
        order: crate::rpc::server::orderbook::Order,
        order_direction: OrderDirection,
    ) -> Self {
        Self {
            id: 1.to_string(),
            symbol: order.symbol,
            price: order.price.into(),
            direction: order_direction,
            quantity: 0.0,
            remaining: 0.0,
            filled: 0.0,
            crt_time: Utc::now().to_string(),
            ex: None,
        }
    }
}

/// order status
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    // waiting for deal
    Pending,
    // order partial deal
    Partial,
    // order complete deal
    Filled,
    // order has been canceled
    Cancelled,
}

impl Default for OrderStatus {
    fn default() -> Self {
        OrderStatus::Pending
    }
}
