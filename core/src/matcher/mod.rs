mod day;
mod gtc;
mod iceberg;
mod ioc_fok;
mod limit;
mod market;
mod stop;
mod stop_limit;

use crossbeam::epoch::{self};
pub use day::DayOrderHandler;
pub use gtc::GTCOrderHandler;
pub use iceberg::IcebergOrderHandler;
pub use ioc_fok::ImmediateOrderHandler;
pub use limit::LimitOrderHandler;
pub use market::MarketOrderHandler;
pub use stop::StopOrderHandler;
pub use stop_limit::StopLimitOrderHandler;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Instant, SystemTime};

use crate::orderbook::OrderTree;
use crate::orderbook::order::{Order, OrderDirection, OrderStatus, OrderType};

pub mod match_utils {
    use super::*;

    pub fn get_orders_at_price(
        tree: &OrderTree,
        price: u64,
        guard: &epoch::Guard,
    ) -> Vec<Arc<Order>> {
        let mut orders = Vec::new();
        let mut current = tree.head.load(Ordering::Relaxed, guard);
        while let Some(node) = unsafe { current.as_ref() } {
            if node.price == price {
                if let Some(order_queue) =
                    unsafe { node.orders.load(Ordering::Relaxed, guard).as_ref() }
                {
                    let mut order_node = order_queue.head.load(Ordering::Relaxed, guard);
                    while let Some(node_ref) = unsafe { order_node.as_ref() } {
                        orders.push(node_ref.order.clone());
                        order_node = node_ref.next.load(Ordering::Relaxed, guard);
                    }
                }
                break;
            }
            current = node.forward[0].load(Ordering::Relaxed, guard);
        }
        orders
    }

    pub fn get_best_prices(bids: &OrderTree, asks: &OrderTree) -> (Option<u64>, Option<u64>) {
        let guard = &epoch::pin();
        let bid_levels = bids.get_price_levels(1);
        let ask_levels = asks.get_price_levels(1);
        let best_bid = bid_levels.get(0).map(|level| level.price as u64);
        let best_ask = ask_levels.get(0).map(|level| level.price as u64);
        (best_bid, best_ask)
    }
}

#[derive(Debug, Clone)]
pub struct OrderMatchContext {
    pub order: Arc<Order>,
    pub price: u64,
    pub is_bid: bool,
    pub added_time: Instant,
    pub match_attempts: u32,
    pub last_match_time: Instant,
    pub next_iceberg_slice_time: Option<Instant>,
    pub iceberg_remaining: f64,
    pub iceberg_display_size: f64,
    pub stop_triggered: bool,
    pub stop_limit_active: bool,
}

impl OrderMatchContext {
    pub fn new(order: Arc<Order>, price: u64, is_bid: bool) -> Self {
        let now = Instant::now();
        let iceberg_display_size = if order.order_type == OrderType::Iceberg {
            // Default iceberg order display ratio is 10%
            order.quantity.load(Ordering::Relaxed) * 0.1
        } else {
            0.0
        };
        OrderMatchContext {
            order,
            price,
            is_bid,
            added_time: now,
            match_attempts: 0,
            last_match_time: now,
            next_iceberg_slice_time: None,
            iceberg_remaining: 0.0,
            iceberg_display_size,
            stop_triggered: false,
            stop_limit_active: false,
        }
    }
}

/// Order handler trait
pub trait OrderHandler: Send + Sync {
    /// Handle order addition to matching engine
    fn add_order(
        &self,
        order: Arc<Order>,
        price: u64,
        is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String>;

    /// Handle order matching logic
    fn process_matching(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String>;

    /// Get the order type supported by this handler
    fn get_order_type(&self) -> OrderType;
}

/// Get order handler for order type
pub fn get_order_handler(order_type: OrderType) -> Box<dyn OrderHandler + Send + Sync> {
    match order_type {
        OrderType::Limit => Box::new(LimitOrderHandler),
        OrderType::Market => Box::new(MarketOrderHandler),
        OrderType::Stop => Box::new(StopOrderHandler),
        OrderType::StopLimit => Box::new(StopLimitOrderHandler),
        OrderType::FOK | OrderType::IOC => Box::new(ImmediateOrderHandler),
        OrderType::Iceberg => Box::new(IcebergOrderHandler),
        OrderType::DAY => Box::new(DayOrderHandler),
        OrderType::GTC => Box::new(GTCOrderHandler),
    }
}

/// Matching engine statistics
#[derive(Debug)]
pub struct MatchStats {
    pub total_matches: AtomicU64,
    pub total_volume: AtomicU64,
    pub total_notional: AtomicU64,
    pub cross_matches: AtomicU64,
    pub price_improvements: AtomicU64,
    pub match_latency_ns: AtomicU64,
    pub orders_processed: AtomicU64,
    pub iceberg_slices: AtomicU64,
    pub stop_triggers: AtomicU64,
}

impl Default for MatchStats {
    fn default() -> Self {
        MatchStats {
            total_matches: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
            total_notional: AtomicU64::new(0),
            cross_matches: AtomicU64::new(0),
            price_improvements: AtomicU64::new(0),
            match_latency_ns: AtomicU64::new(0),
            orders_processed: AtomicU64::new(0),
            iceberg_slices: AtomicU64::new(0),
            stop_triggers: AtomicU64::new(0),
        }
    }
}

/// Match result
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub match_id: u64,
    pub timestamp_ns: u64,
    pub symbol: Arc<str>,
    pub buyer_order_id: String,
    pub seller_order_id: String,
    pub price: u64,
    pub quantity: u64,
    pub bid_price: u64,
    pub ask_price: u64,
    pub is_cross: bool,
}

impl MatchResult {
    pub fn new(
        match_id: u64,
        symbol: Arc<str>,
        buyer_order_id: String,
        seller_order_id: String,
        price: u64,
        quantity: u64,
        bid_price: u64,
        ask_price: u64,
        is_cross: bool,
    ) -> Self {
        let timestamp_ns = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        MatchResult {
            match_id,
            timestamp_ns,
            symbol,
            buyer_order_id,
            seller_order_id,
            price,
            quantity,
            bid_price,
            ask_price,
            is_cross,
        }
    }
}

/// Price level matcher
#[derive(Debug)]
struct PriceLevelMatcher {
    price: u64,
    bid_quantity: AtomicU64,
    ask_quantity: AtomicU64,
    bid_orders: Vec<Arc<Order>>,
    ask_orders: Vec<Arc<Order>>,
    last_match_time: Instant,
}

impl PriceLevelMatcher {
    fn new(price: u64) -> Self {
        PriceLevelMatcher {
            price,
            bid_quantity: AtomicU64::new(0),
            ask_quantity: AtomicU64::new(0),
            bid_orders: Vec::new(),
            ask_orders: Vec::new(),
            last_match_time: Instant::now(),
        }
    }
}

/// Matching engine
pub(crate) struct MatchEngine {
    // Order queues
    pending_limit_orders: parking_lot::Mutex<VecDeque<OrderMatchContext>>,
    market_order_queue: parking_lot::Mutex<VecDeque<Arc<Order>>>,
    stop_order_queue: parking_lot::Mutex<VecDeque<Arc<Order>>>,
    stop_limit_order_queue: parking_lot::Mutex<VecDeque<Arc<Order>>>,
    // IOC/FOK orders need immediate processing
    immediate_order_queue: parking_lot::Mutex<Vec<Arc<Order>>>,
    // Iceberg order management
    iceberg_orders: parking_lot::Mutex<HashMap<String, OrderMatchContext>>,
    // DAY order expiration check
    day_orders: parking_lot::Mutex<HashMap<String, Instant>>,
    // Price level matchers
    price_matchers: parking_lot::RwLock<HashMap<u64, PriceLevelMatcher>>,
    // Match result callback (optional)
    match_callback: parking_lot::Mutex<Option<Arc<dyn Fn(MatchResult) + Send + Sync>>>,
    // Statistics
    stats: MatchStats,
    // Sequence generator
    match_id_counter: AtomicU64,
    // Running status
    is_running: AtomicBool,
}

impl MatchEngine {
    /// Create new matching engine
    pub fn new() -> Self {
        MatchEngine {
            pending_limit_orders: parking_lot::Mutex::new(VecDeque::new()),
            market_order_queue: parking_lot::Mutex::new(VecDeque::new()),
            stop_order_queue: parking_lot::Mutex::new(VecDeque::new()),
            stop_limit_order_queue: parking_lot::Mutex::new(VecDeque::new()),
            immediate_order_queue: parking_lot::Mutex::new(Vec::new()),
            iceberg_orders: parking_lot::Mutex::new(HashMap::new()),
            day_orders: parking_lot::Mutex::new(HashMap::new()),
            price_matchers: parking_lot::RwLock::new(HashMap::new()),
            match_callback: parking_lot::Mutex::new(None),
            stats: MatchStats::default(),
            match_id_counter: AtomicU64::new(1),
            is_running: AtomicBool::new(true),
        }
    }

    /// Add order to matching engine (asynchronous version)
    pub(crate) fn add_order(
        &self,
        order: Arc<Order>,
        price: u64,
        is_bid: bool,
    ) -> Result<(), String> {
        // Fast validation
        if order.quantity.load(Ordering::Relaxed) <= 0.0 {
            return Err("Invalid order quantity".to_string());
        }

        // Fast dispatch based on order type
        match order.order_type {
            OrderType::Market => {
                let mut queue = self.market_order_queue.lock();
                queue.push_back(order.clone());
            }
            OrderType::IOC | OrderType::FOK => {
                let mut queue = self.immediate_order_queue.lock();
                queue.push(order.clone());
            }
            OrderType::Stop => {
                let mut queue = self.stop_order_queue.lock();
                queue.push_back(order.clone());
            }
            OrderType::StopLimit => {
                let mut queue = self.stop_limit_order_queue.lock();
                queue.push_back(order.clone());
            }
            OrderType::Iceberg => {
                let mut map = self.iceberg_orders.lock();
                let context = OrderMatchContext::new(order.clone(), price, is_bid);
                map.insert(order.id.clone(), context);
            }
            OrderType::DAY => {
                let mut map = self.day_orders.lock();
                map.insert(order.id.clone(), Instant::now());
                // For limit orders, add to pending queue
                if price > 0 {
                    let mut pending = self.pending_limit_orders.lock();
                    pending.push_back(OrderMatchContext::new(order.clone(), price, is_bid));
                }
            }
            _ => {
                // Limit, GTC, etc.
                if price > 0 {
                    let mut pending = self.pending_limit_orders.lock();
                    pending.push_back(OrderMatchContext::new(order.clone(), price, is_bid));
                }
            }
        }

        // Fast update statistics
        self.stats.orders_processed.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Add to price level matcher
    pub(crate) fn add_to_price_matcher(&self, order: &Arc<Order>, price: u64, is_bid: bool) {
        let mut price_matchers = self.price_matchers.write();
        let matcher = price_matchers
            .entry(price)
            .or_insert_with(|| PriceLevelMatcher::new(price));
        let quantity = order.quantity.load(Ordering::Relaxed) as u64;
        if is_bid {
            matcher.bid_quantity.fetch_add(quantity, Ordering::Relaxed);
            matcher.bid_orders.push(order.clone());
        } else {
            matcher.ask_quantity.fetch_add(quantity, Ordering::Relaxed);
            matcher.ask_orders.push(order.clone());
        }
    }

    pub(crate) fn execute_price_discovery(
        &self,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) {
        let start_time = Instant::now();

        // First, check for cross matches
        let (best_bid, best_ask) = match_utils::get_best_prices(bids, asks);
        if let (Some(best_bid), Some(best_ask)) = (best_bid, best_ask) {
            if best_bid >= best_ask {
                let handler = LimitOrderHandler;
                handler.execute_cross_matching(self, bids, asks, symbol, best_bid, best_ask);
                self.stats.cross_matches.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Process different order types in priority order
        let handlers: Vec<Box<dyn OrderHandler>> = vec![
            Box::new(ImmediateOrderHandler), // Highest priority: IOC/FOK
            Box::new(MarketOrderHandler),    // Market orders
            Box::new(LimitOrderHandler),     // Limit orders
            Box::new(StopOrderHandler),      // Stop orders
            Box::new(StopLimitOrderHandler), // Stop-limit orders
            Box::new(IcebergOrderHandler),   // Iceberg orders
            Box::new(DayOrderHandler),       // DAY orders
            Box::new(GTCOrderHandler),       // GTC orders
        ];

        for handler in handlers {
            if !self.is_running.load(Ordering::Relaxed) {
                break;
            }

            if let Err(e) = handler.process_matching(self, bids, asks, symbol) {
                // Log error but continue with other handlers
                eprintln!(
                    "Error processing {:?} orders: {}",
                    handler.get_order_type(),
                    e
                );
            }
        }

        // Update latency statistics
        let latency_ns = start_time.elapsed().as_nanos() as u64;
        self.stats
            .match_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);
    }

    /// Execute a single match (for handlers to call)
    pub fn execute_single_match(
        &self,
        aggressive: &Arc<Order>,
        passive: &Arc<Order>,
        price: u64,
        quantity: f64,
        symbol: &Arc<str>,
        bid_price: u64,
        ask_price: u64,
        is_cross: bool,
    ) {
        // Generate match ID
        let match_id = self.match_id_counter.fetch_add(1, Ordering::SeqCst);

        // Update statistics
        self.stats.total_matches.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_volume
            .fetch_add(quantity as u64, Ordering::Relaxed);
        self.stats
            .total_notional
            .fetch_add((price as f64 * quantity) as u64, Ordering::Relaxed);

        // Create match result
        let match_result = MatchResult::new(
            match_id,
            symbol.clone(),
            if aggressive.direction == OrderDirection::Buy {
                aggressive.id.clone()
            } else {
                passive.id.clone()
            },
            if aggressive.direction == OrderDirection::Sell {
                aggressive.id.clone()
            } else {
                passive.id.clone()
            },
            price,
            quantity as u64,
            bid_price,
            ask_price,
            is_cross,
        );

        // Update order status
        aggressive.filled.fetch_add(quantity, Ordering::SeqCst);
        aggressive.remaining.fetch_sub(quantity, Ordering::SeqCst);
        passive.filled.fetch_add(quantity, Ordering::SeqCst);
        passive.remaining.fetch_sub(quantity, Ordering::SeqCst);

        // Check if orders are fully filled
        if aggressive.remaining.load(Ordering::Relaxed) <= 0.0 {
            aggressive
                .status
                .store(OrderStatus::Filled, Ordering::SeqCst);
        } else {
            aggressive
                .status
                .store(OrderStatus::Partial, Ordering::SeqCst);
        }

        if passive.remaining.load(Ordering::Relaxed) <= 0.0 {
            passive.status.store(OrderStatus::Filled, Ordering::SeqCst);
        } else {
            passive.status.store(OrderStatus::Partial, Ordering::SeqCst);
        }

        // Call match result callback (if set)
        if let Some(callback) = self.match_callback.lock().as_ref() {
            callback(match_result);
        }
    }

    /// Set match result callback
    pub fn set_match_callback<F>(&self, callback: F)
    where
        F: Fn(MatchResult) + Send + Sync + 'static,
    {
        *self.match_callback.lock() = Some(Arc::new(callback));
    }

    /// Process new order (for external calls)
    pub fn process_order(&self, order: Arc<Order>) -> Result<(), String> {
        let price = order.price.load(Ordering::Relaxed) as u64;
        let is_bid = order.direction == OrderDirection::Buy;
        self.add_order(order, price, is_bid)
    }

    /// Stop matching engine
    pub(crate) fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }

    /// Get statistics
    pub(crate) fn get_stats(&self) -> MatchStats {
        MatchStats {
            total_matches: AtomicU64::new(self.stats.total_matches.load(Ordering::Relaxed)),
            total_volume: AtomicU64::new(self.stats.total_volume.load(Ordering::Relaxed)),
            total_notional: AtomicU64::new(self.stats.total_notional.load(Ordering::Relaxed)),
            cross_matches: AtomicU64::new(self.stats.cross_matches.load(Ordering::Relaxed)),
            price_improvements: AtomicU64::new(
                self.stats.price_improvements.load(Ordering::Relaxed),
            ),
            match_latency_ns: AtomicU64::new(self.stats.match_latency_ns.load(Ordering::Relaxed)),
            orders_processed: AtomicU64::new(self.stats.orders_processed.load(Ordering::Relaxed)),
            iceberg_slices: AtomicU64::new(self.stats.iceberg_slices.load(Ordering::Relaxed)),
            stop_triggers: AtomicU64::new(self.stats.stop_triggers.load(Ordering::Relaxed)),
        }
    }

    /// Get pending limit orders queue (for handlers)
    pub fn get_pending_limit_orders(&self) -> parking_lot::MutexGuard<VecDeque<OrderMatchContext>> {
        self.pending_limit_orders.lock()
    }

    /// Get market order queue (for handlers)
    pub fn get_market_order_queue(&self) -> parking_lot::MutexGuard<VecDeque<Arc<Order>>> {
        self.market_order_queue.lock()
    }

    /// Get stop order queue (for handlers)
    pub fn get_stop_order_queue(&self) -> parking_lot::MutexGuard<VecDeque<Arc<Order>>> {
        self.stop_order_queue.lock()
    }

    /// Get stop-limit order queue (for handlers)
    pub fn get_stop_limit_order_queue(&self) -> parking_lot::MutexGuard<VecDeque<Arc<Order>>> {
        self.stop_limit_order_queue.lock()
    }

    /// Get IOC/FOK order queue (for handlers)
    pub fn get_immediate_order_queue(&self) -> parking_lot::MutexGuard<Vec<Arc<Order>>> {
        self.immediate_order_queue.lock()
    }

    /// Get iceberg orders map (for handlers)
    pub fn get_iceberg_orders(
        &self,
    ) -> parking_lot::MutexGuard<HashMap<String, OrderMatchContext>> {
        self.iceberg_orders.lock()
    }

    /// Get DAY orders map (for handlers)
    pub fn get_day_orders(&self) -> parking_lot::MutexGuard<HashMap<String, Instant>> {
        self.day_orders.lock()
    }

    /// Get price matchers (for handlers)
    pub fn get_price_matchers(
        &self,
    ) -> parking_lot::RwLockReadGuard<HashMap<u64, PriceLevelMatcher>> {
        self.price_matchers.read()
    }

    /// Get running status
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    /// Get statistics reference (for handlers)
    pub fn get_stats_ref(&self) -> &MatchStats {
        &self.stats
    }
}

// Manual Debug implementation, skip closure field
impl std::fmt::Debug for MatchEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug_struct = f.debug_struct("MatchEngine");
        debug_struct.field("pending_limit_orders", &self.pending_limit_orders);
        debug_struct.field("market_order_queue", &self.market_order_queue);
        debug_struct.field("stop_order_queue", &self.stop_order_queue);
        debug_struct.field("stop_limit_order_queue", &self.stop_limit_order_queue);
        debug_struct.field("immediate_order_queue", &self.immediate_order_queue);
        debug_struct.field("iceberg_orders", &self.iceberg_orders);
        debug_struct.field("day_orders", &self.day_orders);
        debug_struct.field("price_matchers", &self.price_matchers);
        debug_struct.field("stats", &self.stats);
        debug_struct.field("match_id_counter", &self.match_id_counter);
        debug_struct.field("is_running", &self.is_running);
        debug_struct.field(
            "match_callback",
            &if self.match_callback.lock().is_some() {
                "Some(<closure>)"
            } else {
                "None"
            },
        );
        debug_struct.finish()
    }
}

impl Default for MatchEngine {
    fn default() -> Self {
        Self::new()
    }
}
