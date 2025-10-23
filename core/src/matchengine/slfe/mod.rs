pub mod iceberg_manager;
pub mod processor;
pub mod sharding;
pub mod status;
/// Sharded lock-free event order book matching engine.
use crossbeam::channel::{Receiver, Sender, unbounded};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Instant,
};
use tokio::join;

use crate::market::{MarketDepth, MarketDepthSnapshot};
use crate::matchengine::slfe::iceberg_manager::IcebergOrderManager;
use crate::matchengine::slfe::processor::OrderProcessor;
use crate::matchengine::slfe::processor::day::DAYOrderProcessor;
use crate::matchengine::slfe::processor::limit::LimitOrderProcessor;
use crate::matchengine::slfe::sharding::{OrderLocation, OrderTreeSharding};
use crate::matchengine::slfe::status::SlfeStats;
use crate::matchengine::tool::math::{
    atomic_to_f64, cal_ewma, cal_mid_price, cal_price_change, f64_to_atomic,
};
use crate::matchengine::tool::slfe::cal_total_quote_value_for_ordertree;
use crate::matchengine::{MatchEvent, MatchResult};
use crate::order::{Order, OrderDirection};
use crate::price::PriceLevel;
use crate::types::{UnifiedError, UnifiedResult};
use crate::{
    matchengine::MatchEngineConfig,
    price::{AskPrice, BidPrice, Price},
};

/// Order book matching engine (Slfe)
///
/// The SLFE struct represents a complete matching engine that maintains separate
/// order books for bids and asks, processes matching events, and tracks order locations.
///
/// # Field
///
/// * bids: Sharded order tree for buy orders (BidPrice), providing concurrent access
/// * asks: Sharded order tree for sell orders (AskPrice), providing concurrent access  
/// * tx/rx: Channel for broadcasting match events to subscribers
/// * order_location: Fast concurrent mapping from order IDs to their locations in the books
/// * stats: Runtime statistics protected by read-write locks
/// * config: Engine configuration parameters
///
#[derive(Debug)]
pub struct Slfe {
    pub bids: Arc<OrderTreeSharding<BidPrice>>,
    pub asks: Arc<OrderTreeSharding<AskPrice>>,
    pub tx: Sender<MatchEvent>,
    pub rx: Arc<Receiver<MatchEvent>>,
    pub order_location: Arc<DashMap<String, OrderLocation>>,
    pub stats: Arc<RwLock<SlfeStats>>,
    pub config: Arc<RwLock<MatchEngineConfig>>,
    // current price
    pub current_price: Arc<AtomicU64>,
    // last match price
    pub last_match_price: Arc<AtomicU64>,
    // mid price
    pub mid_price: Arc<AtomicU64>,
    // iceberg order manager
    pub iceberg: Arc<IcebergOrderManager>,
    // day order handler
    pub day_order_handler: Arc<DAYOrderProcessor>,
}

impl Slfe {
    /// create a new slfe match engine
    pub fn new(config: Option<MatchEngineConfig>) -> Self {
        let (tx, rx) = unbounded();
        let config = config.unwrap();
        Self {
            bids: Arc::new(OrderTreeSharding::new(config.shard_count)),
            asks: Arc::new(OrderTreeSharding::new(config.shard_count)),
            tx: tx,
            rx: Arc::new(rx),
            // The mapping table of order positions in shards
            // key: order id, value: order location
            order_location: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(SlfeStats::new())),
            config: Arc::new(RwLock::new(config)),
            // current price
            current_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
            // last match price
            last_match_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
            // mid price
            mid_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
            // iceberg order manager
            iceberg: Arc::new(IcebergOrderManager::new()),
            // day order handler
            day_order_handler: Arc::new(DAYOrderProcessor::new()),
        }
    }

    /// start engine
    pub async fn start_engine(self: Arc<Self>) {
        let mut tasks = Vec::new();
        // ---------------------- core event ----------------------
        let event_matcher = self.clone();
        let event_handle = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(event_matcher.start_event_engine())
        });
        // add event handle
        tasks.push(event_handle);
        // -------------------- iceberg manager -------------------
        let iceberg_self_arc_clone = self.clone();
        let iceberg_arc_clone = self.iceberg.clone();
        let iceberg_manager = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(iceberg_arc_clone.start_manager(iceberg_self_arc_clone))
        });
        // add iceberg handle
        tasks.push(iceberg_manager);
        // -------------------- day order handler -------------------
        let day_order_self_arc_clone = self.clone();
        tasks.push(tokio::spawn(async move {
            day_order_self_arc_clone
                .clone()
                .day_order_handler
                .start_expiry_cleanup(day_order_self_arc_clone)
                .await
        }));
        // --------------------- depth monitor ---------------------
        let depth_self_arc_clone = self.clone();
        // add depth monitor
        tasks.push(tokio::spawn(async move {
            depth_self_arc_clone.start_depth_monitor().await;
        }));
        for task in tasks {
            join!(task);
        }
    }

    /// start event engine
    async fn start_event_engine(self: Arc<Self>) {
        let mut event_batch = Vec::<MatchEvent>::with_capacity(self.config.read().batch_size);
        let mut last_process_time = Instant::now();
        let process_interval = Duration::from_micros(self.config.read().match_interval);
        let matcher = self.clone();
        loop {
            while let Ok(event) = matcher.rx.try_recv() {
                event_batch.push(event);
                if event_batch.len() >= matcher.config.read().batch_size {
                    let matcher = self.clone();
                    matcher.handle_event_batch(&event_batch).await;
                    event_batch.clear();
                    last_process_time = Instant::now();
                }
            }
            if !event_batch.is_empty() && last_process_time.elapsed() >= process_interval {
                let matcher = self.clone();
                matcher.handle_event_batch(&event_batch).await;
                event_batch.clear();
                last_process_time = Instant::now();
            }
            if event_batch.is_empty() {
                self.try_continuous_match().await;
            }
            tokio::time::sleep(Duration::from_micros(10)).await;
        }
    }

    async fn start_depth_monitor(self: Arc<Self>) {
        let mut last_adjustment = Instant::now();
        let adjustment_interval = Duration::from_secs(2);
        let s = self.clone();
        loop {
            let depth = MarketDepth::from_slfe(self.clone()).await;
            // update mid price
            if let Some(mid_price) = s.calculate_mid_price() {
                s.mid_price
                    .store(f64_to_atomic(mid_price), Ordering::Relaxed);
            }
            if s.clone().config.read().enable_auto_tuning
                && last_adjustment.elapsed() >= adjustment_interval
            {
                s.clone().auto_tuning(&depth).await;
                last_adjustment = Instant::now();
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn try_continuous_match(&self) {
        let mut match_occurred = true;
        let mut total_matched = 0;
        while match_occurred {
            match_occurred = false;
            if let Some(results) = LimitOrderProcessor::handle(self).await {
                if !results.is_empty() {
                    match_occurred = true;
                    total_matched += results.len();
                    // update current price and last match price
                    if let Some(last_price) = results.last().map(|r| r.price) {
                        self.current_price
                            .store(f64_to_atomic(last_price), Ordering::Relaxed);
                        self.last_match_price
                            .store(f64_to_atomic(last_price), Ordering::Relaxed);
                    }
                    self.notify_match_results(results).await;
                }
            }
            if total_matched > 1000 {
                break;
            }
        }
        // update mid price
        if let Some(mid_price) = self.calculate_mid_price() {
            self.mid_price
                .store(f64_to_atomic(mid_price), Ordering::Relaxed);
        }
    }

    /// Test function, may be deleted in the future.
    async fn notify_match_results(&self, results: Vec<MatchResult>) {
        for result in results {
            if result.quantity > 0.0 {
                self.current_price
                    .store(f64_to_atomic(result.price), Ordering::Relaxed);
                self.last_match_price
                    .store(f64_to_atomic(result.price), Ordering::Relaxed);
                if let Some(mid_price) = self.calculate_mid_price() {
                    self.mid_price
                        .store(f64_to_atomic(mid_price), Ordering::Relaxed);
                }
                println!(
                    "Matched: {} @ {} (bid: {}, ask: {})",
                    result.quantity,
                    result.price,
                    &result.bid_order_id[..8],
                    &result.ask_order_id[..8]
                );
            }
        }
    }

    /// Handle order cancellation events.
    async fn handle_cancellation(self: Arc<Self>, order_id: &str) {
        if let Some(location) = self.order_location.get(order_id) {
            match location.direction {
                OrderDirection::Buy => {
                    self.bids.remove_order(order_id, location.shard_id);
                }
                OrderDirection::Sell => {
                    self.asks.remove_order(order_id, location.shard_id);
                }
                OrderDirection::None => (),
            }
            self.order_location.remove(order_id);
        }
    }

    /// Used to batch process all events in a blocked thread.
    async fn handle_event_batch(self: Arc<Self>, events: &[MatchEvent]) {
        let start_time = Instant::now();
        let mut processed = 0;
        let mut matched = 0;
        let mut total_quantity = 0.0;
        for event in events {
            match event {
                MatchEvent::NewLimitOrder => {
                    processed += 1;
                    let self_clone = self.clone();
                    if let Some(results) = LimitOrderProcessor::handle(&self).await {
                        matched += results.len();
                        total_quantity += results.iter().map(|r| r.quantity).sum::<f64>();
                        let self_clone = self.clone();
                        self_clone.notify_match_results(results).await;
                    }
                }
                MatchEvent::CancelOrder(order_id) => {
                    processed += 1;
                    let self_clone = self.clone();
                    self_clone.handle_cancellation(order_id).await;
                }
                MatchEvent::ImmediateMatch => {
                    if let Some(results) = LimitOrderProcessor::handle(&self).await {
                        matched += results.len();
                        total_quantity += results.iter().map(|r| r.quantity).sum::<f64>();
                        let self_clone = self.clone();
                        self_clone.notify_match_results(results).await;
                    }
                }
                MatchEvent::UpdateConfig {
                    batch_size: _,
                    match_interval: _,
                } => {
                    // Matching engine configuration update event, currently no implementation.
                }
                MatchEvent::Shutdown => {
                    return;
                }
            }
        }
        let self_clone = self.clone();
        self_clone.update_stats(processed, matched, total_quantity, start_time.elapsed());
    }

    /// update stats
    fn update_stats(&self, processed: usize, matched: usize, quantity: f64, latency: Duration) {
        let mut stats = self.stats.write();
        stats.orders_processed += processed as u64;
        stats.orders_matched += matched as u64;
        stats.total_quantity += quantity;
        // -------------- update current price start --------------
        let current_price = self.get_current_price();
        let last_price = atomic_to_f64(stats.last_price_bits);
        if last_price > 0.0 {
            // This is an absolute value and does not take into account the volatility calculation of the trade direction.
            let price_change = cal_price_change(current_price, last_price);
            stats.price_volatility = cal_ewma(stats.price_volatility, price_change);
        }
        stats.last_price_bits = f64_to_atomic(current_price);
        stats.last_price_update = Instant::now();
        // --------------- update current price end ---------------
        let new_latency_us = latency.as_micros() as f64;
        if stats.avg_match_latency_us == 0.0 {
            stats.avg_match_latency_us = new_latency_us;
        } else {
            stats.avg_match_latency_us =
                (stats.avg_match_latency_us * 0.9) + (new_latency_us * 0.1);
        }
        stats.current_queue_depth = self.rx.len();
        let elapsed = stats.last_update.elapsed();
        if elapsed.as_secs_f64() > 0.0 {
            let current_tps = processed as f64 / elapsed.as_secs_f64();
            if current_tps > stats.peak_tps as f64 {
                stats.peak_tps = current_tps as u64;
            }
        }
        stats.last_update = Instant::now();
    }

    /// auto tuning engine performance configuration.
    async fn auto_tuning(self: Arc<Self>, depth: &MarketDepth) {
        if (self.config.read().enable_auto_tuning) {
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

    pub fn calculate_spread(self: Arc<Self>) -> f64 {
        if let (Some(best_bid), Some(best_ask)) =
            (self.bids.get_best_price(), self.asks.get_best_price())
        {
            best_ask.to_f64() - best_bid.to_f64()
        } else {
            0.0
        }
    }

    /// Get a map of all price tiers relative to order quantities.
    pub async fn get_price_level_total_order(
        &self,
        direction: OrderDirection,
    ) -> BTreeMap<u64, usize> {
        match direction {
            OrderDirection::Buy => self.bids.get_price_level_total_order(),
            OrderDirection::Sell => self.asks.get_price_level_total_order(),
            OrderDirection::None => BTreeMap::new(),
        }
    }

    /// Gets a map of the sum of base units for all price levels.
    pub async fn get_price_level_total_base_unit(
        &self,
        direction: OrderDirection,
    ) -> BTreeMap<u64, f64> {
        match direction {
            OrderDirection::Buy => self.bids.get_price_level_total_base_unit(),
            OrderDirection::Sell => self.asks.get_price_level_total_base_unit(),
            OrderDirection::None => BTreeMap::new(),
        }
    }

    /// Get the total value of quotes for all price levels.
    pub async fn get_price_level_total_quote_value(
        &self,
        direction: OrderDirection,
    ) -> BTreeMap<u64, f64> {
        match direction {
            OrderDirection::Buy => self.bids.get_price_level_total_quote_value(),
            OrderDirection::Sell => self.asks.get_price_level_total_quote_value(),
            OrderDirection::None => BTreeMap::new(),
        }
    }

    /// get the quantity of orders with a specified price and a specified trading direction\
    ///
    /// # Return
    /// * usize: order count
    pub async fn get_order_count_by_price(&self, price: f64, direction: OrderDirection) -> usize {
        match direction {
            OrderDirection::Buy => self.bids.get_order_count_by_price(price),
            OrderDirection::Sell => self.asks.get_order_count_by_price(price),
            OrderDirection::None => 0,
        }
    }

    /// Get the total number of Base units available for trading at the specified price and in the specified trading direction (the left unit of the trading pair).
    /// # Return
    /// * f64: total base unit
    pub async fn get_total_base_unit_by_price(&self, price: f64, direction: OrderDirection) -> f64 {
        match direction {
            OrderDirection::Buy => self.bids.get_total_base_unit_by_price(price),
            OrderDirection::Sell => self.asks.get_total_base_unit_by_price(price),
            OrderDirection::None => 0.0,
        }
    }

    /// Get the total value of quote units at a specified price and in a specified trading direction.
    /// # Return
    /// * f64: total quote unit value
    pub async fn get_total_quote_unit_value(&self, price: f64, direction: OrderDirection) -> f64 {
        match direction {
            OrderDirection::Buy => self.bids.get_total_quote_value_by_price(price),
            OrderDirection::Sell => self.asks.get_total_quote_value_by_price(price),
            OrderDirection::None => 0.0,
        }
    }

    /// Get the total value of quote units in a specified trading direction.
    /// # Return
    /// * f64: total quote unit value
    pub async fn get_total_quote_value_all_prices(&self, direction: OrderDirection) -> f64 {
        match direction {
            OrderDirection::Buy => cal_total_quote_value_for_ordertree(&self.bids),
            OrderDirection::Sell => cal_total_quote_value_for_ordertree(&self.asks),
            OrderDirection::None => 0.0,
        }
    }

    /// get the number of orders in the specified trading direction.
    pub async fn get_total_order_count_by_direction(
        &self,
        direction: Option<OrderDirection>,
    ) -> usize {
        match direction {
            Some(OrderDirection::Buy) => self.bids.get_total_order_count(),
            Some(OrderDirection::Sell) => self.asks.get_total_order_count(),
            Some(OrderDirection::None) => 0,
            None => self.bids.get_total_order_count() + self.asks.get_total_order_count(),
        }
    }

    /// get market depth snapshot
    pub async fn get_market_depth_snapshot(&self, levels: Option<usize>) -> MarketDepthSnapshot {
        let bid_distribution = self.bids.get_price_level_total_base_unit();
        let ask_distribution = self.asks.get_price_level_total_base_unit();
        let mut bid_levels: Vec<PriceLevel> = bid_distribution
            .iter()
            .rev()
            .map(|(&price_key, &quantity)| PriceLevel {
                price: price_key as f64 / 10000.0,
                quantity,
                order_count: self
                    .bids
                    .get_order_count_by_price(price_key as f64 / 10000.0),
            })
            .collect();
        let mut ask_levels: Vec<PriceLevel> = ask_distribution
            .iter()
            .map(|(&price_key, &quantity)| PriceLevel {
                price: price_key as f64 / 10000.0,
                quantity,
                order_count: self
                    .asks
                    .get_order_count_by_price(price_key as f64 / 10000.0),
            })
            .collect();
        // Interception depth level
        if let Some(depth_levels) = levels {
            if bid_levels.len() > depth_levels {
                bid_levels.truncate(depth_levels);
            }
            if ask_levels.len() > depth_levels {
                ask_levels.truncate(depth_levels);
            }
        }
        MarketDepthSnapshot {
            bids: bid_levels,
            asks: ask_levels,
            timestamp: Instant::now(),
            total_bid_orders: self.bids.get_total_order_count(),
            total_ask_orders: self.asks.get_total_order_count(),
        }
    }

    /// get current price
    pub fn get_current_price(&self) -> f64 {
        atomic_to_f64(self.current_price.load(Ordering::Relaxed))
    }

    /// get last match price
    pub fn get_last_match_price(&self) -> f64 {
        atomic_to_f64(self.last_match_price.load(Ordering::Relaxed))
    }

    /// get mid price
    pub fn get_mid_price(&self) -> f64 {
        atomic_to_f64(self.mid_price.load(Ordering::Relaxed))
    }

    /// calculate mid price
    fn calculate_mid_price(&self) -> Option<f64> {
        if let (Some(best_bid), Some(best_ask)) =
            (self.bids.get_best_price(), self.asks.get_best_price())
        {
            Some(cal_mid_price(best_bid.to_f64(), best_ask.to_f64()))
        } else {
            None
        }
    }

    /// update current price
    pub fn update_current_price(&self, price: f64) -> Result<(), UnifiedError> {
        if price <= 0.0 {
            return Err(UnifiedError::UpdateCurrentPrice(
                "Price must be greater than 0".to_string(),
            ));
        }
        self.current_price
            .store(f64_to_atomic(price), Ordering::Relaxed);
        Ok(())
    }

    /// add order
    pub async fn add_order(&self, order: Order) -> UnifiedResult<String> {
        OrderProcessor::handle_new_order(self, order).await
    }
}
