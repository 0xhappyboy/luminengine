use std::time::Instant;

use crate::matchengine::tool::math::f64_to_atomic;

#[derive(Debug)]
pub struct SlfeStats {
    pub orders_processed: u64,
    pub orders_matched: u64,
    pub total_quantity: f64,
    pub avg_match_latency_us: f64,
    pub peak_tps: u64,
    pub current_queue_depth: usize,
    pub last_update: Instant,
    pub price_volatility: f64, // Price volatility, regardless of trading direction
    pub last_price_bits: u64,  // The last price in bits
    pub last_price_update: Instant, // Last price update time
}

impl SlfeStats {
    pub fn new() -> Self {
        Self {
            orders_processed: 0,
            orders_matched: 0,
            total_quantity: 0.0,
            avg_match_latency_us: 0.0,
            peak_tps: 0,
            current_queue_depth: 0,
            last_update: Instant::now(),
            price_volatility: 0.0,
            last_price_bits: f64_to_atomic(0.0),
            last_price_update: Instant::now(),
        }
    }
}
