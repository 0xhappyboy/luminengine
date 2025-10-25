pub mod manager;
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
use crate::matchengine::slfe::manager::event::EventManager;
use crate::matchengine::slfe::manager::expired::ExpiredOrderManager;
use crate::matchengine::slfe::manager::gtc::GTCOrderManager;
use crate::matchengine::slfe::manager::iceberg::IcebergOrderManager;
use crate::matchengine::slfe::manager::price::PriceManager;
use crate::matchengine::slfe::manager::status::StatusManager;
use crate::matchengine::slfe::sharding::{OrderLocation, OrderTreeSharding};
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
    // order records the position in the slice mapping.
    pub order_location: Arc<DashMap<String, OrderLocation>>,
    pub config: Arc<RwLock<MatchEngineConfig>>,
    // current price
    pub current_price: Arc<AtomicU64>,
    // last match price
    pub last_match_price: Arc<AtomicU64>,
    // mid price
    pub mid_price: Arc<AtomicU64>,
    // iceberg order manager
    pub iceberg_manager: Arc<IcebergOrderManager>,
    // gtc order manager
    pub gtc_manager: Arc<GTCOrderManager>,
    // core event manager
    pub event_manager: Arc<EventManager>,
    // status manager
    pub status_manager: Arc<StatusManager>,
    // price manager
    pub price_manager: Arc<PriceManager>,
}

impl Slfe {
    /// create a new slfe match engine
    pub fn new(config: Option<MatchEngineConfig>) -> Self {
        let config = config.unwrap();
        Self {
            bids: Arc::new(OrderTreeSharding::new(config.shard_count)),
            asks: Arc::new(OrderTreeSharding::new(config.shard_count)),
            // The mapping table of order positions in shards
            // key: order id, value: order location
            order_location: Arc::new(DashMap::new()),
            config: Arc::new(RwLock::new(config)),
            // current price
            current_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
            // last match price
            last_match_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
            // mid price
            mid_price: Arc::new(AtomicU64::new(f64_to_atomic(0.0f64))),
            // iceberg order manager
            iceberg_manager: Arc::new(IcebergOrderManager::new()),
            // gtc order manager
            gtc_manager: Arc::new(GTCOrderManager::new()),
            // event manager
            event_manager: Arc::new(EventManager::new()),
            // status manager
            status_manager: Arc::new(StatusManager::new()),
            // price manager
            price_manager: Arc::new(PriceManager::new()),
        }
    }

    /// start engine
    pub async fn start_engine(self: Arc<Self>) -> UnifiedResult<String> {
        let mut tasks = Vec::new();
        // ---------------------- core event ----------------------
        let event_manager_slfe_arc_clone = Arc::clone(&self);
        let event_manager_arc_clone = self.event_manager.clone();
        let event_handle = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(
                event_manager_arc_clone.start_event_manager(event_manager_slfe_arc_clone),
            );
        });
        // add event handle
        tasks.push(event_handle);
        // -------------------- iceberg manager -------------------
        let iceberg_self_arc_clone = Arc::clone(&self);
        let iceberg_arc_clone = self.iceberg_manager.clone();
        let iceberg_manager = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(iceberg_arc_clone.start_iceberg_manager(iceberg_self_arc_clone))
        });
        // add iceberg manager
        tasks.push(iceberg_manager);
        // -------------------- gtc manager -------------------
        let gtc_arc_clone = self.gtc_manager.clone();
        let gtc_manager = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(gtc_arc_clone.start_gtc_manager())
        });
        // add gtc manager
        tasks.push(gtc_manager);
        // -------------------- price manager -------------------
        // price listener
        let price_listener_tx = self.price_manager.tx.clone();
        let price_listener_slfe_arc_clone = self.clone();
        let price_listener_running_arc_clone = self.price_manager.is_running.clone();
        let handle_price_listener = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(PriceManager::price_listener_task(
                price_listener_slfe_arc_clone,
                price_listener_tx,
                price_listener_running_arc_clone,
            ));
        });
        // handle stop
        let stop_rx = self.price_manager.rx.clone();
        let sopt_slfe_arc_clone = self.clone();
        let sopt_running_arc_clone = self.price_manager.is_running.clone();
        // Market-filled orders
        let stop_orders = Arc::clone(&self.price_manager.stop_orders);
        let buy_stop_orders = Arc::clone(&self.price_manager.buy_stop_orders);
        let sell_stop_orders = Arc::clone(&self.price_manager.sell_stop_orders);
        let handle_stop = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(PriceManager::handle_stop_order(
                stop_orders,
                buy_stop_orders,
                sell_stop_orders,
                stop_rx,
                sopt_slfe_arc_clone,
                sopt_running_arc_clone,
            ));
        });
        // handle stop limit
        let stop_limit_rx = self.price_manager.rx.clone();
        let sopt_limit_slfe_arc_clone = self.clone();
        let sopt_limit_running_arc_clone = self.price_manager.is_running.clone();
        // Limit order
        let stop_limit_orders = Arc::clone(&self.price_manager.stop_limit_orders);
        let buy_stop_limit_orders = Arc::clone(&self.price_manager.buy_stop_limit_orders);
        let sell_stop_limit_orders = Arc::clone(&self.price_manager.sell_stop_limit_orders);
        let handle_stop_limit = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(PriceManager::handle_stop_limit_order(
                stop_limit_orders,
                buy_stop_limit_orders,
                sell_stop_limit_orders,
                stop_limit_rx,
                sopt_limit_slfe_arc_clone,
                sopt_limit_running_arc_clone,
            ));
        });
        tasks.push(handle_price_listener);
        tasks.push(handle_stop);
        tasks.push(handle_stop_limit);
        // -------------------- day order handler -------------------
        let day_order_self_arc_clone = Arc::clone(&self);
        tasks.push(tokio::spawn(async move {
            ExpiredOrderManager::start_expiry_order_manager(day_order_self_arc_clone).await
        }));
        // --------------------- depth monitor ---------------------
        let depth_self_arc_clone = Arc::clone(&self);
        // add depth monitor
        tasks.push(tokio::spawn(async move {
            depth_self_arc_clone.start_depth_monitor().await;
        }));
        for task in tasks {
            join!(task);
        }
        Ok("Slfe Engine Started".to_string())
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

    /// Get the number of transactions available in the shard
    pub async fn get_available_quantity_for_order(&self, order: &Order) -> f64 {
        match order.direction {
            OrderDirection::Buy => {
                let sorted_ask_prices = self.asks.get_all_ask_prices_sorted();
                let mut available = 0.0;
                for ask_price in sorted_ask_prices {
                    if ask_price.to_f64() > order.price
                        && order.order_type == crate::order::OrderType::Limit
                    {
                        break;
                    }
                    for shard_id in 0..self.config.read().shard_count {
                        let shard = self.asks.shards[shard_id].read();
                        if let Some(orders) = shard.tree.get(&ask_price) {
                            for counter_order in orders {
                                if counter_order.can_trade() {
                                    available += counter_order.remaining;
                                }
                            }
                        }
                    }
                }
                available
            }
            OrderDirection::Sell => {
                let sorted_bid_prices = self.bids.get_all_bid_prices_sorted();
                let mut available = 0.0;
                for bid_price in sorted_bid_prices {
                    if bid_price.to_f64() < order.price
                        && order.order_type == crate::order::OrderType::Limit
                    {
                        break;
                    }
                    for shard_id in 0..self.config.read().shard_count {
                        let shard = self.bids.shards[shard_id].read();
                        if let Some(orders) = shard.tree.get(&bid_price) {
                            for counter_order in orders {
                                if counter_order.can_trade() {
                                    available += counter_order.remaining;
                                }
                            }
                        }
                    }
                }
                available
            }
            OrderDirection::None => 0.0,
        }
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

    // add order
    // pub async fn add_order(&self, order: Order) -> UnifiedResult<String> {
    //     OrderProcessor::handle_new_order(self, order).await
    // }
}
