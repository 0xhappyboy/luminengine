use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::matchengine::{
    slfe::Slfe,
    tool::math::{atomic_to_f64, cal_ewma, cal_price_change, f64_to_atomic},
};

#[derive(Debug)]
pub struct SlfeStatus {
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

impl SlfeStatus {
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

#[derive(Debug)]
pub struct StatusManager {
    pub status: RwLock<SlfeStatus>,
}

impl StatusManager {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(SlfeStatus::new()),
        }
    }

    /// update stats
    pub fn update(
        &self,
        slfe: &Slfe,
        processed: usize,
        matched: usize,
        quantity: f64,
        latency: Duration,
    ) {
        let mut status = self.status.write();
        status.orders_processed += processed as u64;
        status.orders_matched += matched as u64;
        status.total_quantity += quantity;
        // -------------- update current price start --------------
        let current_price = slfe.get_current_price();
        let last_price = atomic_to_f64(status.last_price_bits);
        if last_price > 0.0 {
            // This is an absolute value and does not take into account the volatility calculation of the trade direction.
            let price_change = cal_price_change(current_price, last_price);
            status.price_volatility = cal_ewma(status.price_volatility, price_change);
        }
        status.last_price_bits = f64_to_atomic(current_price);
        status.last_price_update = Instant::now();
        // --------------- update current price end ---------------
        let new_latency_us = latency.as_micros() as f64;
        if status.avg_match_latency_us == 0.0 {
            status.avg_match_latency_us = new_latency_us;
        } else {
            status.avg_match_latency_us =
                (status.avg_match_latency_us * 0.9) + (new_latency_us * 0.1);
        }
        status.current_queue_depth = slfe.event_manager.get_pending_events();
        // status.current_queue_depth = self.rx.len();
        let elapsed = status.last_update.elapsed();
        if elapsed.as_secs_f64() > 0.0 {
            let current_tps = processed as f64 / elapsed.as_secs_f64();
            if current_tps > status.peak_tps as f64 {
                status.peak_tps = current_tps as u64;
            }
        }
        status.last_update = Instant::now();
    }
}
