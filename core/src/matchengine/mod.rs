use std::time::Instant;
pub mod matcher;
pub mod slfe;

/// match engine config
#[derive(Debug, Clone)]
pub struct MatchEngineConfig {
    pub shard_count: usize,       // shard count
    pub batch_size: usize,        // batch size
    pub match_interval: u64,      // match interval, unit: microseconds
    pub max_queue_depth: usize,   // max queue depth
    pub enable_auto_tuning: bool, // enable auto tuning
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

/// Market Depth Information Structure
#[derive(Debug)]
struct MarketDepth {
    bid_order_count: usize, // Total buy orders
    ask_order_count: usize, // Total sell orders
    best_bid: Option<f64>,  // best bid price
    best_ask: Option<f64>,  // Best selling price
    spread: f64,            // Bid-ask spread
    timestamp: Instant,     // Timestamp
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

#[derive(Debug)]
pub enum MatchEngineError {
    AddOrderError(String),
    Error(String),
}
