use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crossbeam::epoch::{self, Guard};

use crate::orderbook::order::{Order, OrderDirection, OrderStatus, OrderType};
use crate::orderbook::{OrderBook, OrderBookStats, OrderTree};

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

/// Matching result
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
    fn new(
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
            .unwrap()
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

/// Order matching context
#[derive(Debug)]
struct OrderMatchContext {
    order: Arc<Order>,
    price: u64,
    is_bid: bool,
    added_time: Instant,
    match_attempts: u32,
    last_match_time: Instant,
    next_iceberg_slice_time: Option<Instant>,
    iceberg_remaining: f64,
    iceberg_display_size: f64,
    stop_triggered: bool,
    stop_limit_active: bool,
}

impl OrderMatchContext {
    fn new(order: Arc<Order>, price: u64, is_bid: bool) -> Self {
        let now = Instant::now();
        let iceberg_display_size = if order.order_type == OrderType::Iceberg {
            // Default iceberg display ratio is 10%
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

/// Cohesive matching engine
pub struct MatchEngine {
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
    // Sequence number generator
    match_id_counter: AtomicU64,
    // Running status
    is_running: AtomicBool,
}

impl MatchEngine {
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

    /// Add order to matching engine
    pub fn add_order(&self, order: Arc<Order>, price: u64, is_bid: bool) -> Result<(), String> {
        let ctx = OrderMatchContext::new(order.clone(), price, is_bid);
        match order.order_type {
            OrderType::Limit => {
                // Limit order: add to pending queue
                self.pending_limit_orders.lock().push_back(ctx);
                // Also add to price level matcher
                self.add_to_price_matcher(&order, price, is_bid);
            }
            OrderType::Market => {
                // Market order: process immediately
                self.market_order_queue.lock().push_back(order);
            }
            OrderType::Stop => {
                // Stop order: wait for trigger
                self.stop_order_queue.lock().push_back(order);
            }
            OrderType::StopLimit => {
                // Stop-limit order: wait for trigger
                self.stop_limit_order_queue.lock().push_back(order);
            }
            OrderType::FOK | OrderType::IOC => {
                // Immediate or cancel order: process immediately
                self.immediate_order_queue.lock().push(order);
            }
            OrderType::Iceberg => {
                // Iceberg order: special handling
                let mut iceberg_map = self.iceberg_orders.lock();
                // Need to clone ctx here since it will be used later
                iceberg_map.insert(order.id.clone(), ctx);

                // Also process as regular limit order
                // Note: cannot use the same ctx again, need to create a new one
                let ctx_for_queue = OrderMatchContext::new(order.clone(), price, is_bid);
                self.pending_limit_orders.lock().push_back(ctx_for_queue);
                self.add_to_price_matcher(&order, price, is_bid);
            }
            OrderType::DAY => {
                // DAY order: set expiration time
                let mut day_map = self.day_orders.lock();
                let expiry = Instant::now() + Duration::from_secs(24 * 60 * 60);
                day_map.insert(order.id.clone(), expiry);

                // Process as regular limit order
                self.pending_limit_orders.lock().push_back(ctx);
                self.add_to_price_matcher(&order, price, is_bid);
            }
            OrderType::GTC => {
                // GTC order: no expiration time
                self.pending_limit_orders.lock().push_back(ctx);
                self.add_to_price_matcher(&order, price, is_bid);
            }
        }
        self.stats.orders_processed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Add to price level matcher
    fn add_to_price_matcher(&self, order: &Arc<Order>, price: u64, is_bid: bool) {
        let mut price_matchers = self.price_matchers.write();
        let matcher = price_matchers
            .entry(price)
            .or_insert_with(|| PriceLevelMatcher::new(price));
        if is_bid {
            matcher.bid_quantity.fetch_add(
                order.quantity.load(Ordering::Relaxed) as u64,
                Ordering::Relaxed,
            );
            matcher.bid_orders.push(order.clone());
        } else {
            matcher.ask_quantity.fetch_add(
                order.quantity.load(Ordering::Relaxed) as u64,
                Ordering::Relaxed,
            );
            matcher.ask_orders.push(order.clone());
        }
    }

    /// Execute price discovery and matching
    pub fn execute_price_discovery(&self, bids: &OrderTree, asks: &OrderTree, symbol: &Arc<str>) {
        let start_time = Instant::now();
        // 1. Get best bid and ask prices
        let guard = &epoch::pin();
        let bid_levels = bids.get_price_levels(1);
        let ask_levels = asks.get_price_levels(1);
        if bid_levels.is_empty() || ask_levels.is_empty() {
            return;
        }
        let best_bid = bid_levels[0].price;
        let best_ask = ask_levels[0].price;
        // 2. Check for price crossing (tradable)
        if best_bid >= best_ask {
            // Cross market found, execute matching
            self.execute_cross_matching(best_bid, best_ask, bids, asks, symbol);
            self.stats.cross_matches.fetch_add(1, Ordering::Relaxed);
        }
        // 3. Process market orders
        self.process_market_orders(bids, asks, symbol);
        // 4. Process limit order matching
        self.process_limit_orders(bids, asks, symbol);
        // 5. Process IOC/FOK orders
        self.process_immediate_orders(bids, asks, symbol);
        // 6. Check stop order trigger conditions
        self.check_stop_orders(best_bid, best_ask);
        // 7. Check DAY order expiration
        self.check_day_order_expiry();
        // 8. Process iceberg order slices
        self.process_iceberg_slices();
        // Record matching latency
        let latency_ns = start_time.elapsed().as_nanos() as u64;
        self.stats
            .match_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);
    }

    /// Execute cross market matching
    fn execute_cross_matching(
        &self,
        best_bid: u64,
        best_ask: u64,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) {
        // Match price: take mid-price or better price
        let match_price = if best_bid == best_ask {
            best_bid
        } else {
            // Price improvement: take price more favorable to the aggressive side
            // Simplified here as mid-price, actual exchanges have different rules
            (best_bid + best_ask) / 2
        };
        let guard = &epoch::pin();
        // Get orders at cross price for both sides
        let bid_orders = self.get_orders_at_price(bids, best_bid, guard);
        let ask_orders = self.get_orders_at_price(asks, best_ask, guard);
        // Match based on price-time priority
        let mut bid_idx = 0;
        let mut ask_idx = 0;
        while bid_idx < bid_orders.len() && ask_idx < ask_orders.len() {
            let bid_order = &bid_orders[bid_idx];
            let ask_order = &ask_orders[ask_idx];
            let bid_qty = bid_order.quantity.load(Ordering::Relaxed);
            let ask_qty = ask_order.quantity.load(Ordering::Relaxed);
            let bid_remaining = bid_order.remaining.load(Ordering::Relaxed);
            let ask_remaining = ask_order.remaining.load(Ordering::Relaxed);
            // Skip fully filled orders
            if bid_remaining <= 0.0 {
                bid_idx += 1;
                continue;
            }
            if ask_remaining <= 0.0 {
                ask_idx += 1;
                continue;
            }
            // Calculate matchable quantity
            let match_qty = bid_remaining.min(ask_remaining);
            if match_qty > 0.0 {
                // Execute match
                self.execute_single_match(
                    bid_order,
                    ask_order,
                    match_price,
                    match_qty,
                    symbol,
                    best_bid,
                    best_ask,
                    true, // is_cross
                );
                self.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .total_volume
                    .fetch_add(match_qty as u64, Ordering::Relaxed);
                self.stats
                    .total_notional
                    .fetch_add((match_qty * match_price as f64) as u64, Ordering::Relaxed);
            }
            // Move to next order
            if bid_remaining <= ask_remaining {
                bid_idx += 1;
            }
            if ask_remaining <= bid_remaining {
                ask_idx += 1;
            }
        }
    }

    /// Process market orders
    fn process_market_orders(&self, bids: &OrderTree, asks: &OrderTree, symbol: &Arc<str>) {
        let mut market_orders = self.market_order_queue.lock();
        let mut processed_orders = Vec::new();
        for market_order in market_orders.iter() {
            // Market orders match with best counterparty
            let opposite_tree = match market_order.direction {
                OrderDirection::Buy => asks,
                OrderDirection::Sell => bids,
                OrderDirection::None => {
                    processed_orders.push(market_order.clone());
                    continue;
                }
            };
            let guard = &epoch::pin();
            let levels = opposite_tree.get_price_levels(10);
            if levels.is_empty() {
                // No counterparty, market order cannot be filled
                market_order
                    .status
                    .store(OrderStatus::Cancelled, Ordering::SeqCst);
                processed_orders.push(market_order.clone());
                continue;
            }
            // Match starting from best price
            let mut remaining_qty = market_order.remaining.load(Ordering::Relaxed);
            for level in levels {
                if remaining_qty <= 0.0 {
                    break;
                }
                let price = level.price as u64;
                let level_qty = level.quantity as f64;
                // Get orders at this price level
                let orders = self.get_orders_at_price(opposite_tree, price, guard);
                for order in orders {
                    if remaining_qty <= 0.0 {
                        break;
                    }
                    let order_remaining = order.remaining.load(Ordering::Relaxed);
                    if order_remaining <= 0.0 {
                        continue;
                    }
                    let match_qty = remaining_qty.min(order_remaining);
                    if match_qty > 0.0 {
                        // Determine match parties - ensure both branches return same type
                        let aggressive;
                        let passive;
                        match market_order.direction {
                            OrderDirection::Buy => {
                                // Buy market order: market order is aggressive
                                aggressive = market_order;
                                passive = &order;
                            }
                            OrderDirection::Sell => {
                                // Sell market order: counterparty is aggressive
                                aggressive = &order;
                                passive = market_order;
                            }
                            _ => continue,
                        };
                        self.execute_single_match(
                            aggressive, passive, price, match_qty, symbol, price, price,
                            false, // not cross
                        );
                        remaining_qty -= match_qty;
                        self.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                        self.stats
                            .total_volume
                            .fetch_add(match_qty as u64, Ordering::Relaxed);
                        self.stats
                            .total_notional
                            .fetch_add((match_qty * price as f64) as u64, Ordering::Relaxed);
                    }
                }
            }
            // Update market order remaining quantity
            market_order
                .remaining
                .store(remaining_qty, Ordering::SeqCst);
            if remaining_qty <= 0.0 {
                market_order
                    .status
                    .store(OrderStatus::Filled, Ordering::SeqCst);
            } else {
                // Partial fill
                market_order
                    .status
                    .store(OrderStatus::Partial, Ordering::SeqCst);
            }
            processed_orders.push(market_order.clone());
        }
        // Remove processed orders
        market_orders.retain(|order| {
            !processed_orders
                .iter()
                .any(|processed_order| processed_order.id == order.id)
        });
    }

    /// Process limit order matching
    fn process_limit_orders(&self, bids: &OrderTree, asks: &OrderTree, symbol: &Arc<str>) {
        let mut pending_orders = self.pending_limit_orders.lock();
        let mut processed_indices = Vec::new();
        for (idx, ctx) in pending_orders.iter_mut().enumerate() {
            // Check if order can still be matched
            let order = &ctx.order;
            let remaining = order.remaining.load(Ordering::Relaxed);
            if remaining <= 0.0 {
                processed_indices.push(idx);
                continue;
            }
            // Check order status
            let status = order.status.load(Ordering::Relaxed);
            if status == OrderStatus::Cancelled || status == OrderStatus::Expired {
                processed_indices.push(idx);
                continue;
            }
            // Find counterparty
            let opposite_tree = if ctx.is_bid { asks } else { bids };
            let guard = &epoch::pin();
            let levels = opposite_tree.get_price_levels(10);
            for level in levels {
                let opposite_price = level.price as u64;
                // Check if price is matchable
                let price_ok = if ctx.is_bid {
                    // Buy order: bid price >= ask price
                    ctx.price >= opposite_price
                } else {
                    // Sell order: ask price <= bid price
                    ctx.price <= opposite_price
                };
                if !price_ok {
                    continue;
                }
                // Get counterparty orders
                let orders = self.get_orders_at_price(opposite_tree, opposite_price, guard);
                for opposite_order in orders {
                    if remaining <= 0.0 {
                        break;
                    }
                    let opposite_remaining = opposite_order.remaining.load(Ordering::Relaxed);
                    if opposite_remaining <= 0.0 {
                        continue;
                    }
                    let match_qty = remaining.min(opposite_remaining);
                    if match_qty > 0.0 {
                        // Determine match price (passive price priority)
                        let match_price = if ctx.is_bid {
                            // Buy order aggressive, match at seller's price
                            opposite_price
                        } else {
                            // Sell order aggressive, match at buyer's price
                            opposite_price
                        };
                        // Execute match - call directly instead of creating tuple
                        if ctx.is_bid {
                            // Buy order aggressive: order is aggressive, opposite_order is passive
                            self.execute_single_match(
                                order,
                                &opposite_order,
                                match_price,
                                match_qty,
                                symbol,
                                ctx.price,
                                opposite_price,
                                false,
                            );
                        } else {
                            // Sell order aggressive: opposite_order is aggressive, order is passive
                            self.execute_single_match(
                                &opposite_order,
                                order,
                                match_price,
                                match_qty,
                                symbol,
                                ctx.price,
                                opposite_price,
                                false,
                            );
                        }
                        // Update remaining quantity
                        let new_remaining = remaining - match_qty;
                        order.remaining.store(new_remaining, Ordering::SeqCst);
                        if new_remaining <= 0.0 {
                            order.status.store(OrderStatus::Filled, Ordering::SeqCst);
                        } else {
                            order.status.store(OrderStatus::Partial, Ordering::SeqCst);
                        }
                        ctx.match_attempts += 1;
                        ctx.last_match_time = Instant::now();
                        self.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                        self.stats
                            .total_volume
                            .fetch_add(match_qty as u64, Ordering::Relaxed);
                        self.stats
                            .total_notional
                            .fetch_add((match_qty * match_price as f64) as u64, Ordering::Relaxed);
                        // Update iceberg order
                        if order.order_type == OrderType::Iceberg {
                            self.update_iceberg_order(order, match_qty);
                        }
                    }
                }
            }
            // If order is fully filled, mark as processed
            if order.remaining.load(Ordering::Relaxed) <= 0.0 {
                processed_indices.push(idx);
            }
        }
        // Remove processed orders (remove from back to avoid index confusion)
        for &idx in processed_indices.iter().rev() {
            if idx < pending_orders.len() {
                pending_orders.remove(idx);
            }
        }
    }

    /// Process IOC/FOK orders
    fn process_immediate_orders(&self, bids: &OrderTree, asks: &OrderTree, symbol: &Arc<str>) {
        let mut immediate_orders = self.immediate_order_queue.lock();
        let mut indices_to_remove = Vec::new();
        for (index, order) in immediate_orders.iter().enumerate() {
            let remaining = order.remaining.load(Ordering::Relaxed);
            if remaining <= 0.0 {
                indices_to_remove.push(index);
                continue;
            }
            // IOC: immediate fill, cancel remainder
            // FOK: fill all or cancel all
            let opposite_tree = match order.direction {
                OrderDirection::Buy => asks,
                OrderDirection::Sell => bids,
                OrderDirection::None => {
                    order.status.store(OrderStatus::Cancelled, Ordering::SeqCst);
                    indices_to_remove.push(index);
                    continue;
                }
            };
            let guard = &epoch::pin();
            let levels = opposite_tree.get_price_levels(10);
            let order_price = order.price.load(Ordering::Relaxed) as u64;
            let mut matched_qty = 0.0;
            // Check if enough liquidity is available
            let mut total_available = 0.0;
            for level in &levels {
                let level_price = level.price as u64;
                // Check if price is acceptable
                let price_ok = match order.direction {
                    OrderDirection::Buy => order_price >= level_price,
                    OrderDirection::Sell => order_price <= level_price,
                    _ => false,
                };
                if price_ok {
                    total_available += level.quantity as f64;
                }
            }
            // FOK orders need all quantity to be matchable
            if order.order_type == OrderType::FOK && total_available < remaining {
                // Insufficient liquidity, cancel all
                order.status.store(OrderStatus::Cancelled, Ordering::SeqCst);
                indices_to_remove.push(index);
                continue;
            }
            // Execute matching
            for level in levels {
                if matched_qty >= remaining {
                    break;
                }
                let level_price = level.price as u64;
                let price_ok = match order.direction {
                    OrderDirection::Buy => order_price >= level_price,
                    OrderDirection::Sell => order_price <= level_price,
                    _ => false,
                };
                if !price_ok {
                    continue;
                }
                let orders = self.get_orders_at_price(opposite_tree, level_price, guard);
                for opposite_order in orders {
                    if matched_qty >= remaining {
                        break;
                    }
                    let opposite_remaining = opposite_order.remaining.load(Ordering::Relaxed);
                    if opposite_remaining <= 0.0 {
                        continue;
                    }
                    let needed_qty = remaining - matched_qty;
                    let match_qty = needed_qty.min(opposite_remaining);
                    if match_qty > 0.0 {
                        // Execute match - call directly based on order direction
                        match order.direction {
                            OrderDirection::Buy => {
                                // Buy IOC/FOK: order is aggressive
                                self.execute_single_match(
                                    order,
                                    &opposite_order,
                                    level_price,
                                    match_qty,
                                    symbol,
                                    order_price,
                                    level_price,
                                    false,
                                );
                            }
                            OrderDirection::Sell => {
                                // Sell IOC/FOK: opposite_order is aggressive
                                self.execute_single_match(
                                    &opposite_order,
                                    order,
                                    level_price,
                                    match_qty,
                                    symbol,
                                    order_price,
                                    level_price,
                                    false,
                                );
                            }
                            _ => continue,
                        }
                        matched_qty += match_qty;
                        self.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                        self.stats
                            .total_volume
                            .fetch_add(match_qty as u64, Ordering::Relaxed);
                        self.stats
                            .total_notional
                            .fetch_add((match_qty * level_price as f64) as u64, Ordering::Relaxed);
                    }
                }
            }
            // Update order status
            if matched_qty > 0.0 {
                order.filled.fetch_add(matched_qty, Ordering::SeqCst);
                order
                    .remaining
                    .store(remaining - matched_qty, Ordering::SeqCst);
                if matched_qty >= remaining {
                    order.status.store(OrderStatus::Filled, Ordering::SeqCst);
                } else {
                    order.status.store(OrderStatus::Partial, Ordering::SeqCst);
                    // IOC order: cancel remaining portion
                    if order.order_type == OrderType::IOC {
                        order.status.store(OrderStatus::Cancelled, Ordering::SeqCst);
                    }
                }
            } else {
                // No match, cancel order
                order.status.store(OrderStatus::Cancelled, Ordering::SeqCst);
            }
            indices_to_remove.push(index);
        }
        // Remove from back to avoid index confusion
        for &index in indices_to_remove.iter().rev() {
            if index < immediate_orders.len() {
                immediate_orders.remove(index);
            }
        }
    }

    /// Check stop order trigger conditions
    fn check_stop_orders(&self, best_bid: u64, best_ask: u64) {
        let mut stop_orders = self.stop_order_queue.lock();
        let mut triggered_orders = Vec::new();
        for order in stop_orders.iter() {
            let stop_price = order.price.load(Ordering::Relaxed) as u64;
            let market_price = match order.direction {
                OrderDirection::Buy => best_ask, // Buy stop: market price >= stop price
                OrderDirection::Sell => best_bid, // Sell stop: market price <= stop price
                _ => continue,
            };
            let triggered = match order.direction {
                OrderDirection::Buy => market_price >= stop_price,
                OrderDirection::Sell => market_price <= stop_price,
                _ => false,
            };
            if triggered {
                triggered_orders.push(order.clone());
                self.stats.stop_triggers.fetch_add(1, Ordering::Relaxed);
            }
        }
        // Move triggered stop orders to market order queue
        for order in triggered_orders {
            stop_orders.retain(|o| o.id != order.id);
            self.market_order_queue.lock().push_back(order);
        }
        // Check stop-limit orders
        let mut stop_limit_orders = self.stop_limit_order_queue.lock();
        let mut triggered_limit_orders = Vec::new();
        for order in stop_limit_orders.iter() {
            // Stop-limit orders need to parse stop price and limit price
            // Simplified here: use same price
            let stop_price = order.price.load(Ordering::Relaxed) as u64;
            let market_price = match order.direction {
                OrderDirection::Buy => best_ask,
                OrderDirection::Sell => best_bid,
                _ => continue,
            };
            let triggered = match order.direction {
                OrderDirection::Buy => market_price >= stop_price,
                OrderDirection::Sell => market_price <= stop_price,
                _ => false,
            };
            if triggered {
                triggered_limit_orders.push(order.clone());
                self.stats.stop_triggers.fetch_add(1, Ordering::Relaxed);
            }
        }
        // Move triggered stop-limit orders to limit order queue
        for order in triggered_limit_orders {
            stop_limit_orders.retain(|o| o.id != order.id);
            let ctx = OrderMatchContext::new(
                order.clone(),
                order.price.load(Ordering::Relaxed) as u64,
                order.direction == OrderDirection::Buy,
            );
            self.pending_limit_orders.lock().push_back(ctx);
        }
    }

    /// Check DAY order expiration
    fn check_day_order_expiry(&self) {
        let mut day_orders = self.day_orders.lock();
        let now = Instant::now();
        let mut expired_orders = Vec::new();
        for (order_id, expiry) in day_orders.iter() {
            if now >= *expiry {
                expired_orders.push(order_id.clone());
            }
        }
        for order_id in expired_orders {
            day_orders.remove(&order_id);
            // Remove expired orders from pending queue
            let mut pending_orders = self.pending_limit_orders.lock();
            pending_orders.retain(|ctx| ctx.order.id != order_id);
            // Update order status
            // Note: In actual implementation, need to notify via callback or other mechanism
        }
    }

    /// Process iceberg order slices
    fn process_iceberg_slices(&self) {
        let mut iceberg_orders = self.iceberg_orders.lock();
        let mut processed_orders = Vec::new();
        let now = Instant::now();
        for (order_id, ctx) in iceberg_orders.iter_mut() {
            // Check if new slice should be shown
            let should_show_slice = match ctx.next_iceberg_slice_time {
                Some(next_time) => now >= next_time,
                None => true, // First display
            };
            if should_show_slice && ctx.iceberg_remaining > 0.0 {
                // Show new slice
                let slice_qty = ctx.iceberg_display_size.min(ctx.iceberg_remaining);
                // Update order display quantity
                ctx.order.quantity.store(slice_qty, Ordering::SeqCst);
                ctx.iceberg_remaining -= slice_qty;
                // Set next display time (e.g., show every 100 milliseconds)
                ctx.next_iceberg_slice_time = Some(now + Duration::from_millis(100));
                self.stats.iceberg_slices.fetch_add(1, Ordering::Relaxed);
            }
            // If iceberg is fully melted, mark as processed
            if ctx.iceberg_remaining <= 0.0 {
                processed_orders.push(order_id.clone());
            }
        }
        // Remove processed iceberg orders
        for order_id in processed_orders {
            iceberg_orders.remove(&order_id);
        }
    }

    /// Update iceberg order
    fn update_iceberg_order(&self, order: &Arc<Order>, matched_qty: f64) {
        let mut iceberg_orders = self.iceberg_orders.lock();
        if let Some(ctx) = iceberg_orders.get_mut(&order.id) {
            // Reduce display quantity
            let current_qty = order.quantity.load(Ordering::Relaxed);
            let new_qty = current_qty - matched_qty;
            if new_qty <= 0.0 && ctx.iceberg_remaining > 0.0 {
                // Current slice fully matched, prepare next slice
                let next_slice = ctx.iceberg_display_size.min(ctx.iceberg_remaining);
                order.quantity.store(next_slice, Ordering::SeqCst);
                ctx.iceberg_remaining -= next_slice;
                ctx.next_iceberg_slice_time = Some(Instant::now() + Duration::from_millis(100));
            } else {
                order.quantity.store(new_qty, Ordering::SeqCst);
            }
        }
    }

    /// Get all orders at specified price
    fn get_orders_at_price(&self, tree: &OrderTree, price: u64, guard: &Guard) -> Vec<Arc<Order>> {
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

    /// Execute single match
    fn execute_single_match(
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
        // Check if order is fully filled
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
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}

impl MatchEngine {
    pub fn get_stats(&self) -> MatchStats {
        // Create a new MatchStats struct and fill with current atomic counter values
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
}

// Manual Debug implementation to skip closure field
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
