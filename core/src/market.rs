use std::{sync::Arc, time::Instant};

use crate::{
    matchengine::slfe::{self, Slfe},
    price::{Price, PriceLevel},
};

/// Market Depth Information Structure
#[derive(Debug)]
pub struct MarketDepth {
    pub bid_order_count: usize, // Total buy orders
    pub ask_order_count: usize, // Total sell orders
    pub best_bid: Option<f64>,  // best bid price
    pub best_ask: Option<f64>,  // Best selling price
    pub spread: f64,            // Bid-ask spread
    pub timestamp: Instant,     // Timestamp
}

impl MarketDepth {
    pub async fn from_slfe(slfe: Arc<Slfe>) -> Self {
        MarketDepth {
            bid_order_count: slfe.bids.get_total_order_count(),
            ask_order_count: slfe.asks.get_total_order_count(),
            best_bid: slfe.bids.get_best_price().map(|p| p.to_f64()),
            best_ask: slfe.asks.get_best_price().map(|p| p.to_f64()),
            spread: slfe.calculate_spread(),
            timestamp: Instant::now(),
        }
    }
}

/// Depth of Market Snapshot Abstract.
#[derive(Debug, Clone)]
pub struct MarketDepthSnapshot {
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub timestamp: Instant,
    pub total_bid_orders: usize,
    pub total_ask_orders: usize,
}

impl MarketDepthSnapshot {
    pub fn spread(&self) -> f64 {
        if let (Some(best_bid), Some(best_ask)) = (self.bids.first(), self.asks.first()) {
            best_ask.price - best_bid.price
        } else {
            0.0
        }
    }
    pub fn get_volume_at_price(&self, price: f64) -> (f64, f64) {
        let bid_volume = self
            .bids
            .iter()
            .find(|level| (level.price - price).abs() < f64::EPSILON)
            .map(|level| level.quantity)
            .unwrap_or(0.0);
        let ask_volume = self
            .asks
            .iter()
            .find(|level| (level.price - price).abs() < f64::EPSILON)
            .map(|level| level.quantity)
            .unwrap_or(0.0);

        (bid_volume, ask_volume)
    }
    pub fn top_bids(&self, n: usize) -> &[PriceLevel] {
        let end = std::cmp::min(n, self.bids.len());
        &self.bids[0..end]
    }
    pub fn top_asks(&self, n: usize) -> &[PriceLevel] {
        let end = std::cmp::min(n, self.asks.len());
        &self.asks[0..end]
    }
}
