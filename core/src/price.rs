/// This module for handling financial price types with specialized ordering semantics.
///
/// This module provides two main price types:
/// - BidPrice: Represents bid prices (buy orders) with descending ordering
/// - AskPrice: Represents ask prices (sell orders) with ascending ordering
///
/// # Ordering Behavior
///
/// The ordering implementations ensure that:
/// - Higher bid prices have higher priority (come first in sorted collections)
/// - Lower ask prices have higher priority (come first in sorted collections)
///
use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
pub trait Price: 'static {
    /// Creates a new price instance from a floating-point value.
    ///
    /// # Arguments
    /// * price - f64 type price
    ///
    /// # Examples
    /// ```
    /// use price::{Price, BidPrice, AskPrice};
    ///
    /// let bid = BidPrice::new(100.50);
    /// let ask = AskPrice::new(100.75);
    /// ```
    fn new(price: f64) -> Self;
    fn to_f64(&self) -> f64;
}

/// Order Bid Price
/// Represents a bid price in a trading system.
///
/// Bid prices are what buyers are willing to pay for an asset.
/// They are ordered in descending order, meaning higher bids
/// Have higher priority and will be processed first.
///
/// # Rules
///
/// - A bid of 100.0 is considered greater than a bid of 99.0
/// - Higher bids will appear first when sorted
///
#[derive(Debug, Clone)]
pub struct BidPrice {
    pub price: f64,
}

impl Ord for BidPrice {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .price
            .partial_cmp(&self.price)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for BidPrice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for BidPrice {
    fn eq(&self, other: &Self) -> bool {
        self.price == other.price
    }
}

impl Eq for BidPrice {}

impl Price for BidPrice {
    fn new(price: f64) -> Self {
        BidPrice { price: price }
    }

    fn to_f64(&self) -> f64 {
        self.price
    }
}

/// Order Ask price
/// Represents an ask price in a trading system.
///
/// Ask prices are what sellers are willing to accept for an asset.
/// Are ordered in ascending order, meaning lower asks.
/// Have higher priority and will be processed first.
///
/// # Ordering Rules
///
/// - An ask of 100.0 is considered less than an ask of 101.0
/// - Lower asks will appear first when sorted
///
#[derive(Debug, Clone)]
pub struct AskPrice {
    pub price: f64,
}

impl Ord for AskPrice {
    fn cmp(&self, other: &Self) -> Ordering {
        self.price
            .partial_cmp(&other.price)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for AskPrice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for AskPrice {
    fn eq(&self, other: &Self) -> bool {
        self.price == other.price
    }
}

impl Eq for AskPrice {}

impl Price for AskPrice {
    fn new(price: f64) -> Self {
        AskPrice { price: price }
    }
    fn to_f64(&self) -> f64 {
        self.price
    }
}

/// Price Level, an abstraction for each price level in the order book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub quantity: f64,
    pub order_count: usize,
}
