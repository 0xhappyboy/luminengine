use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::{
    matchengine::{
        MatchResult,
        slfe::Slfe,
        tool::math::{cal_mid_price, f64_to_atomic},
    },
    price::{AskPrice, BidPrice, Price},
};

/// Unified Price Information Manager
#[derive(Debug)]
pub struct PriceInfoManager {
    // current price
    pub current_price: Arc<AtomicU64>,
    // last match price
    pub last_match_price: Arc<AtomicU64>,
    // mid price
    pub mid_price: Arc<AtomicU64>,
}

impl PriceInfoManager {
    pub fn new() -> Self {
        Self {
            // current price
            current_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
            // last match price
            last_match_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
            // mid price
            mid_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
        }
    }
    // Update order book price info
    pub async fn update_price(&self, slfe: Arc<Slfe>, results: Vec<MatchResult>) {
        for result in results {
            if result.quantity > 0.0 {
                self.current_price
                    .store(f64_to_atomic(result.price), Ordering::Relaxed);
                self.last_match_price
                    .store(f64_to_atomic(result.price), Ordering::Relaxed);
                if let Some(mid_price) = self.cal_mid_price(slfe.clone()) {
                    self.mid_price
                        .store(f64_to_atomic(mid_price), Ordering::Relaxed);
                }
            }
        }
    }
    /// calculate mid price
    pub fn cal_mid_price(&self, slfe: Arc<Slfe>) -> Option<f64> {
        if let (Some(best_bid), Some(best_ask)) =
            (slfe.bids.get_best_price(), slfe.asks.get_best_price())
        {
            Some(cal_mid_price(best_bid.to_f64(), best_ask.to_f64()))
        } else {
            None
        }
    }

    /// get bids best price
    pub fn get_bids_best_price(&self, slfe: Arc<Slfe>) -> Option<BidPrice> {
        slfe.bids.get_best_price()
    }
    /// get asks best price
    pub fn get_asks_best_price(&self, slfe: Arc<Slfe>) -> Option<AskPrice> {
        slfe.asks.get_best_price()
    }
}
