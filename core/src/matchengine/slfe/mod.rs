pub mod manager;
pub mod processor;
pub mod sharding;
pub mod status;
/// Sharded lock-free event order book matching engine.
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::{collections::BTreeMap, sync::Arc, time::Instant};
use tokio::join;

use crate::market::{MarketDepth, MarketDepthSnapshot};
use crate::matchengine::MatchEvent;
use crate::matchengine::slfe::manager::config::ConfigManager;
use crate::matchengine::slfe::manager::event::EventManager;
use crate::matchengine::slfe::manager::expired::ExpiredOrderManager;
use crate::matchengine::slfe::manager::gtc::GTCOrderManager;
use crate::matchengine::slfe::manager::iceberg::{
    IcebergOrderEvent, IcebergOrderManager, IcebergOrderStatus,
};
use crate::matchengine::slfe::manager::price_change::{PriceChangeManager, StopOrderStatus};
use crate::matchengine::slfe::manager::price_info::PriceInfoManager;
use crate::matchengine::slfe::manager::status::{SlfeStatus, StatusManager};
use crate::matchengine::slfe::processor::OrderProcessor;
use crate::matchengine::slfe::processor::ioc::IOCOrderProcessor;
use crate::matchengine::slfe::processor::stop::StopOrderProcessor;
use crate::matchengine::slfe::processor::stoplimit::StopLimitOrderProcessor;
use crate::matchengine::slfe::sharding::{OrderLocation, OrderTreeSharding};
use crate::matchengine::tool::math::{atomic_to_f64, f64_to_atomic};
use crate::matchengine::tool::slfe::cal_total_quote_value_for_ordertree;
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
    // config manager
    pub config_manager: Arc<ConfigManager>,
    // price info manager
    pub price_info_manager: Arc<PriceInfoManager>,
    // iceberg order manager
    pub iceberg_manager: Arc<IcebergOrderManager>,
    // gtc order manager
    pub gtc_manager: Arc<GTCOrderManager>,
    // core event manager
    pub event_manager: Arc<EventManager>,
    // status manager
    pub status_manager: Arc<StatusManager>,
    // price change manager
    pub price_change_manager: Arc<PriceChangeManager>,
    // expired order manager
    pub expired_order_manager: Arc<ExpiredOrderManager>,
}

impl Slfe {
    /// create a new slfe match engine
    pub fn new(config: Option<MatchEngineConfig>) -> Self {
        let config = config.unwrap_or(MatchEngineConfig::default());
        Self {
            bids: Arc::new(OrderTreeSharding::new(config.shard_count)),
            asks: Arc::new(OrderTreeSharding::new(config.shard_count)),
            // The mapping table of order positions in shards
            // key: order id, value: order location
            order_location: Arc::new(DashMap::new()),
            // config manager
            config_manager: Arc::new(ConfigManager::new(config)),
            // price info manager
            price_info_manager: Arc::new(PriceInfoManager::new()),
            // iceberg order manager
            iceberg_manager: Arc::new(IcebergOrderManager::new()),
            // gtc order manager
            gtc_manager: Arc::new(GTCOrderManager::new()),
            // event manager
            event_manager: Arc::new(EventManager::new()),
            // status manager
            status_manager: Arc::new(StatusManager::new()),
            // price change manager
            price_change_manager: Arc::new(PriceChangeManager::new()),
            // expired order manager
            expired_order_manager: Arc::new(ExpiredOrderManager::new()),
        }
    }

    /// start engine
    pub async fn start_engine(self: Arc<Self>) -> UnifiedResult<String> {
        let mut tasks = Vec::new();
        // ---------------------- core event ----------------------
        let event_manager_slfe_arc_clone = Arc::clone(&self);
        let event_manager_arc_clone = self.event_manager.clone();
        let event_handle = tokio::task::spawn_blocking(move || {
            event_manager_arc_clone.start_event_manager(event_manager_slfe_arc_clone);
        });
        // add event handle
        tasks.push(event_handle);
        // -------------------- iceberg manager -------------------
        let iceberg_self_arc_clone = Arc::clone(&self);
        let iceberg_arc_clone = self.iceberg_manager.clone();
        let iceberg_manager = tokio::task::spawn_blocking(move || {
            iceberg_arc_clone.start_iceberg_manager(iceberg_self_arc_clone);
        });
        // add iceberg manager
        tasks.push(iceberg_manager);
        // -------------------- gtc manager -------------------
        let gtc_arc_clone = self.gtc_manager.clone();
        let gtc_manager = tokio::task::spawn_blocking(move || gtc_arc_clone.start_gtc_manager());
        // add gtc manager
        tasks.push(gtc_manager);
        // -------------------- price manager -------------------
        // price listener
        let price_listener_tx = self.price_change_manager.tx.clone();
        let price_listener_slfe_arc_clone = self.clone();
        let price_listener_running_arc_clone = self.price_change_manager.is_running.clone();
        let handle_price_listener = tokio::task::spawn_blocking(move || {
            price_listener_slfe_arc_clone
                .clone()
                .price_change_manager
                .price_listener_task(
                    price_listener_slfe_arc_clone,
                    price_listener_tx,
                    price_listener_running_arc_clone,
                );
        });
        // handle stop
        let stop_rx = self.price_change_manager.rx.clone();
        let sopt_slfe_arc_clone = self.clone();
        let sopt_running_arc_clone = self.price_change_manager.is_running.clone();
        // Market-filled orders
        let stop_orders = Arc::clone(&self.price_change_manager.stop_orders);
        let buy_stop_orders = Arc::clone(&self.price_change_manager.buy_stop_orders);
        let sell_stop_orders = Arc::clone(&self.price_change_manager.sell_stop_orders);
        let handle_stop = tokio::task::spawn_blocking(move || {
            sopt_slfe_arc_clone
                .clone()
                .price_change_manager
                .handle_stop_order(
                    stop_orders,
                    buy_stop_orders,
                    sell_stop_orders,
                    stop_rx,
                    sopt_slfe_arc_clone,
                    sopt_running_arc_clone,
                );
        });
        // handle stop limit
        let stop_limit_rx = self.price_change_manager.rx.clone();
        let sopt_limit_slfe_arc_clone = self.clone();
        let sopt_limit_running_arc_clone = self.price_change_manager.is_running.clone();
        // Limit order
        let stop_limit_orders = Arc::clone(&self.price_change_manager.stop_limit_orders);
        let buy_stop_limit_orders = Arc::clone(&self.price_change_manager.buy_stop_limit_orders);
        let sell_stop_limit_orders = Arc::clone(&self.price_change_manager.sell_stop_limit_orders);
        let handle_stop_limit = tokio::task::spawn_blocking(move || {
            sopt_limit_slfe_arc_clone
                .clone()
                .price_change_manager
                .handle_stop_limit_order(
                    stop_limit_orders,
                    buy_stop_limit_orders,
                    sell_stop_limit_orders,
                    stop_limit_rx,
                    sopt_limit_slfe_arc_clone,
                    sopt_limit_running_arc_clone,
                );
        });
        tasks.push(handle_price_listener);
        tasks.push(handle_stop);
        tasks.push(handle_stop_limit);
        // -------------------- expired order manager -------------------
        let expired_order_manager_self_arc_clone = Arc::clone(&self);
        let expired_order_manager = tokio::task::spawn_blocking(move || {
            expired_order_manager_self_arc_clone
                .clone()
                .expired_order_manager
                .start_expiry_order_manager(expired_order_manager_self_arc_clone);
        });
        tasks.push(expired_order_manager);
        // --------------------- price info manager ---------------------
        let price_info_manager_self_arc_clone = Arc::clone(&self);
        let price_info_manager = tokio::task::spawn_blocking(move || {
            price_info_manager_self_arc_clone
                .price_info_manager
                .start_price_manager(price_info_manager_self_arc_clone.clone())
        });
        tasks.push(price_info_manager);
        // ---------------------  config manager ---------------------
        let config_manager_self_arc_clone = Arc::clone(&self);
        let config_manager = tokio::task::spawn_blocking(move || {
            config_manager_self_arc_clone
                .config_manager
                .clone()
                .start_config_manager(config_manager_self_arc_clone);
        });
        tasks.push(config_manager);
        for task in tasks {
            let _ = join!(task);
        }
        Ok("Slfe Engine Started".to_string())
    }

    /// Get the number of transactions available in the shard
    pub fn get_available_quantity_for_order(&self, order: &Order) -> f64 {
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
                    for shard_id in 0..self.config_manager.config.read().shard_count {
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
                    for shard_id in 0..self.config_manager.config.read().shard_count {
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
    pub fn get_price_level_total_order(&self, direction: OrderDirection) -> BTreeMap<u64, usize> {
        match direction {
            OrderDirection::Buy => self.bids.get_price_level_total_order(),
            OrderDirection::Sell => self.asks.get_price_level_total_order(),
            OrderDirection::None => BTreeMap::new(),
        }
    }

    /// Gets a map of the sum of base units for all price levels.
    pub fn get_price_level_total_base_unit(&self, direction: OrderDirection) -> BTreeMap<u64, f64> {
        match direction {
            OrderDirection::Buy => self.bids.get_price_level_total_base_unit(),
            OrderDirection::Sell => self.asks.get_price_level_total_base_unit(),
            OrderDirection::None => BTreeMap::new(),
        }
    }

    /// Get the total value of quotes for all price levels.
    pub fn get_price_level_total_quote_value(
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
    pub fn get_order_count_by_price(&self, price: f64, direction: OrderDirection) -> usize {
        match direction {
            OrderDirection::Buy => self.bids.get_order_count_by_price(price),
            OrderDirection::Sell => self.asks.get_order_count_by_price(price),
            OrderDirection::None => 0,
        }
    }

    /// Get the total number of Base units available for trading at the specified price and in the specified trading direction (the left unit of the trading pair).
    /// # Return
    /// * f64: total base unit
    pub fn get_total_base_unit_by_price(&self, price: f64, direction: OrderDirection) -> f64 {
        match direction {
            OrderDirection::Buy => self.bids.get_total_base_unit_by_price(price),
            OrderDirection::Sell => self.asks.get_total_base_unit_by_price(price),
            OrderDirection::None => 0.0,
        }
    }

    /// Get the total value of quote units at a specified price and in a specified trading direction.
    /// # Return
    /// * f64: total quote unit value
    pub fn get_total_quote_unit_value(&self, price: f64, direction: OrderDirection) -> f64 {
        match direction {
            OrderDirection::Buy => self.bids.get_total_quote_value_by_price(price),
            OrderDirection::Sell => self.asks.get_total_quote_value_by_price(price),
            OrderDirection::None => 0.0,
        }
    }

    /// Get the total value of quote units in a specified trading direction.
    /// # Return
    /// * f64: total quote unit value
    pub fn get_total_quote_value_all_prices(&self, direction: OrderDirection) -> f64 {
        match direction {
            OrderDirection::Buy => cal_total_quote_value_for_ordertree(&self.bids),
            OrderDirection::Sell => cal_total_quote_value_for_ordertree(&self.asks),
            OrderDirection::None => 0.0,
        }
    }

    /// get the number of orders in the specified trading direction.
    pub fn get_total_order_count_by_direction(&self, direction: Option<OrderDirection>) -> usize {
        match direction {
            Some(OrderDirection::Buy) => self.bids.get_total_order_count(),
            Some(OrderDirection::Sell) => self.asks.get_total_order_count(),
            Some(OrderDirection::None) => 0,
            None => self.bids.get_total_order_count() + self.asks.get_total_order_count(),
        }
    }

    /// get market depth snapshot
    pub fn get_market_depth_snapshot(&self, levels: Option<usize>) -> MarketDepthSnapshot {
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
        atomic_to_f64(
            self.price_info_manager
                .current_price
                .load(Ordering::Relaxed),
        )
    }

    /// get last match price
    pub fn get_last_match_price(&self) -> f64 {
        atomic_to_f64(
            self.price_info_manager
                .last_match_price
                .load(Ordering::Relaxed),
        )
    }

    /// get mid price
    pub fn get_mid_price(&self) -> f64 {
        atomic_to_f64(self.price_info_manager.mid_price.load(Ordering::Relaxed))
    }

    /// update current price
    pub fn update_current_price(&self, price: f64) -> Result<(), UnifiedError> {
        if price <= 0.0 {
            return Err(UnifiedError::UpdateCurrentPrice(
                "Price must be greater than 0".to_string(),
            ));
        }
        self.price_info_manager
            .current_price
            .store(f64_to_atomic(price), Ordering::Relaxed);
        Ok(())
    }

    pub fn add_order(self: Arc<Self>, order: Order) -> UnifiedResult<String> {
        OrderProcessor::handle_new_order(self.clone(), order)
    }

    pub fn add_stop_order(
        self: Arc<Self>,
        original_order: Order,
        stop_price: f64,
        expiry_seconds: Option<u64>,
    ) -> UnifiedResult<String> {
        StopOrderProcessor::add_stop_order(self.clone(), original_order, stop_price, expiry_seconds)
    }

    pub fn add_stop_limit_order(
        self: Arc<Self>,
        original_order: Order,
        stop_price: f64,
        limit_price: f64,
        expiry_seconds: Option<u64>,
    ) -> UnifiedResult<String> {
        StopLimitOrderProcessor::add_stop_limit_order(
            self.clone(),
            original_order,
            stop_price,
            limit_price,
            expiry_seconds,
        )
    }

    pub fn cancel_stop_order(&self, order_id: &str) -> UnifiedResult<bool> {
        self.price_change_manager.cancel_stop_order(order_id)
    }

    pub fn modify_stop_order(
        &self,
        order_id: &str,
        new_stop_price: f64,
        new_limit_price: Option<f64>,
    ) -> UnifiedResult<bool> {
        self.price_change_manager
            .set_stop_order(order_id, new_stop_price, new_limit_price)
    }

    pub fn get_gtc_order(&self, order_id: &str) -> Option<Order> {
        self.gtc_manager.get_order(order_id)
    }

    pub fn get_active_gtc_orders(&self) -> Vec<Order> {
        self.gtc_manager.get_active_orders()
    }

    pub fn check_ioc_feasibility(&self, order: &Order) -> (bool, f64) {
        IOCOrderProcessor::check_ioc_feasibility(self, order)
    }

    pub fn get_iceberg_order_status(&self, order_id: &str) -> Option<IcebergOrderStatus> {
        self.iceberg_manager.cache_pool.get_iceberg_order(order_id)
    }

    pub fn cancel_iceberg_order(&self, order_id: &str) -> UnifiedResult<String> {
        self.iceberg_manager
            .event_tx
            .send(IcebergOrderEvent::Cancel {
                order_id: order_id.to_string(),
            })
            .map_err(|_| {
                UnifiedError::IcebergOrderError("Failed to send cancel event".to_string())
            })?;
        Ok("Cancel successful".to_string())
    }

    pub fn get_stop_order_status(&self, order_id: &str) -> Option<StopOrderStatus> {
        self.price_change_manager.get_stop_order_status(order_id)
    }

    pub fn get_engine_status(&self) -> Arc<RwLock<SlfeStatus>> {
        self.status_manager.status.clone()
    }

    pub fn get_order_location(&self, order_id: &str) -> Option<OrderLocation> {
        self.order_location.get(order_id).map(|entry| entry.clone())
    }

    pub fn batch_cancel_orders(self: Arc<Self>, order_ids: &[&str]) {
        for &order_id in order_ids {
            self.clone().cancel_order(order_id);
        }
    }

    pub fn cancel_order(self: Arc<Self>, order_id: &str) {
        OrderProcessor::handle_cancel_order(self.as_ref(), order_id);
    }

    pub fn reload_pending_gtc_orders(&self) -> UnifiedResult<Vec<Order>> {
        self.gtc_manager.load_pending_gtc_orders()
    }

    pub fn get_total_active_orders(&self) -> usize {
        self.bids.get_total_order_count() + self.asks.get_total_order_count()
    }

    pub fn cleanup_expired_orders(self: Arc<Self>) -> UnifiedResult<usize> {
        ExpiredOrderManager::cleanup_expired_orders(self.clone())
    }

    pub fn trigger_immediate_match(&self) -> UnifiedResult<String> {
        self.event_manager.send_event(MatchEvent::ImmediateMatch)
    }

    pub fn get_config(&self) -> MatchEngineConfig {
        self.config_manager.config.read().clone()
    }

    pub fn update_config(&self, new_config: MatchEngineConfig) {
        *self.config_manager.config.write() = new_config;
    }
}
