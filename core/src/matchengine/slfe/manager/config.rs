use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::RwLock;

use crate::{
    market::MarketDepth,
    matchengine::{MatchEngineConfig, slfe::Slfe},
};

#[derive(Debug)]
pub struct ConfigManager {
    pub config: Arc<RwLock<MatchEngineConfig>>,
}

impl ConfigManager {
    pub fn new(config: MatchEngineConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }
    pub fn start_config_manager(self: Arc<Self>, slfe: Arc<Slfe>) {
        let mut last_adjustment = Instant::now();
        let adjustment_interval = Duration::from_secs(2);
        loop {
            let depth = MarketDepth::from_slfe(slfe.clone());
            if self.clone().config.read().enable_auto_tuning
                && last_adjustment.elapsed() >= adjustment_interval
            {
                self.clone().auto_tuning(&depth);
                last_adjustment = Instant::now();
            }
            let _ = tokio::time::sleep(Duration::from_millis(500));
        }
    }

    fn auto_tuning(self: Arc<Self>, depth: &MarketDepth) {
        if self.config.read().enable_auto_tuning {
            let total_orders = depth.bid_order_count + depth.ask_order_count;
            // new batch size
            let batch_size = if total_orders > 1_000_000 {
                200
            } else if total_orders > 100_000 {
                500
            } else {
                1000
            };
            // new match interval
            let match_interval = if depth.spread < depth.best_ask.unwrap_or(1.0) * 0.0005 {
                10
            } else if depth.spread < depth.best_ask.unwrap_or(1.0) * 0.001 {
                25
            } else {
                50
            };
            {
                let mut config = self.config.write();
                config.set_batch_size(batch_size);
                config.set_match_interval(match_interval);
            }
        }
    }
}
