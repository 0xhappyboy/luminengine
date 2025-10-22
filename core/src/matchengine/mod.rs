use std::time::Instant;

use crate::order::Order;

pub mod matcher;
pub mod slfe;
pub mod tool;

/// match engine config
#[derive(Debug, Clone)]
pub struct MatchEngineConfig {
    pub shard_count: usize,       // shard count
    pub batch_size: usize,        // batch size
    pub match_interval: u64,      // match interval, unit: microseconds
    pub max_queue_depth: usize,   // max queue depth
    pub enable_auto_tuning: bool, // enable auto tuning
}

impl MatchEngineConfig {
    pub fn set_shard_count(&mut self, shard_count: usize) {
        self.shard_count = shard_count;
    }
    pub fn set_max_queue_depth(&mut self, batch_size: usize) {
        self.batch_size = batch_size;
    }
    pub fn set_match_interval(&mut self, match_interval: u64) {
        self.match_interval = match_interval;
    }
    pub fn set_batch_size(&mut self, max_queue_depth: usize) {
        self.max_queue_depth = max_queue_depth;
    }
}

impl Default for MatchEngineConfig {
    fn default() -> Self {
        Self {
            shard_count: 64,          // The default is 64 shard
            batch_size: 500,          // The default batch size is 500 events.
            match_interval: 50,       // The default matching interval is 50 microseconds
            max_queue_depth: 100_000, // Maximum queue depth of 100,000 events
            enable_auto_tuning: true, // auto tuning is enabled by default
        }
    }
}

/// Match Result
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub bid_order_id: String,
    pub ask_order_id: String,
    pub price: f64,
    pub quantity: f64,
    pub timestamp: Instant,
}

#[derive(Debug, Clone)]
pub enum MatchEvent {
    NewOrder(Order),
    CancelOrder(String), // order_id
    ImmediateMatch,
    Shutdown,
    UpdateConfig {
        batch_size: usize,
        match_interval: u64,
    },
}

#[derive(Debug)]
pub enum MatchEngineError {
    AddOrderError(String),
    Error(String),
}
