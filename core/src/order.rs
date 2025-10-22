use std::{
    collections::{BTreeMap, VecDeque},
    time::Instant,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::price::Price;
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
    // the order expiration date.
    Expired,
}

impl Default for OrderStatus {
    fn default() -> Self {
        OrderStatus::Pending
    }
}

#[derive(Debug, Clone)]
pub struct OrderTree<P>
where
    P: Price + Ord + PartialOrd + PartialEq + Eq + Clone,
{
    pub tree: BTreeMap<P, VecDeque<Order>>,
    pub total_orders: usize,
}

impl<P> OrderTree<P>
where
    P: Price + Ord + PartialOrd + PartialEq + Eq + Clone,
{
    pub fn new(tree: BTreeMap<P, VecDeque<Order>>, total_orders: usize) -> Self {
        Self {
            tree: tree,
            total_orders: total_orders,
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
    pub fn cancel(&mut self, order: Order) {}
}

pub enum OrderType {
    Limit,
    Market,
    Stop,
    StopLimit,
    FOK,
    IOC,
    Iceberg,
}

impl OrderType {}

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
#[derive(Debug, Clone)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub price: f64,
    pub direction: OrderDirection,
    pub quantity: f64,
    pub remaining: f64,
    pub filled: f64,
    pub crt_time: String,
    pub status: OrderStatus,
    pub expiry: Option<Instant>,
    pub ex: Option<String>,
}

impl Order {
    pub fn new(
        id: String,
        symbol: String,
        price: f64,
        quantity: f64,
        status: OrderStatus,
        expiry: Option<Instant>,
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
            status: status,
            expiry: expiry,
            crt_time: Utc::now().to_string(),
            ex: None,
        }
    }
    pub fn can_trade(&self) -> bool {
        if self.remaining <= 0.0 {
            return false;
        }
        match self.status {
            OrderStatus::Cancelled | OrderStatus::Filled => return false,
            OrderStatus::Expired => return false,
            _ => {}
        }
        if let Some(expiry) = self.expiry {
            if Instant::now() > expiry {
                return false;
            }
        }
        true
    }

    pub fn execute_trade(&mut self, quantity: f64) {
        self.remaining -= quantity;
        self.remaining = self.remaining.max(0.0);
        if self.remaining == 0.0 {}
    }

    pub fn from_rpc_order(
        order: crate::net::rpc::server::orderbook::Order,
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
            status: OrderStatus::Pending,
            expiry: Some(Instant::now()),
        }
    }
}

impl Default for Order {
    fn default() -> Self {
        Self {
            id: "1".to_string(),
            symbol: "ETH".to_string(),
            price: 15.1,
            direction: OrderDirection::Buy,
            quantity: 0.0,
            remaining: 0.0,
            filled: 0.0,
            crt_time: Utc::now().to_string(),
            ex: None,
            status: OrderStatus::Pending,
            expiry: Some(Instant::now()),
        }
    }
}
