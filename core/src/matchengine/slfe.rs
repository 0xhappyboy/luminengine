/// Sharded lock-free event order book matching engine.
use crossbeam::channel::{Receiver, Sender, unbounded};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Instant,
};
use tokio::join;

use crate::market::{MarketDepth, MarketDepthSnapshot};
use crate::matchengine::tool::slfe::cal_total_quote_value_for_ordertree;
use crate::matchengine::{MatchEngineError, MatchEvent, MatchResult};
use crate::orderbook::OrderTree;
use crate::price::PriceLevel;
use crate::{
    matchengine::MatchEngineConfig,
    orderbook::{Order, OrderDirection},
    price::{AskPrice, BidPrice, Price},
};

#[derive(Debug, Clone)]
pub struct OrderLocation {
    pub price_key: u64,
    pub direction: OrderDirection,
    pub shard_id: usize,
    pub order_id: String,
}

#[derive(Debug)]
pub struct ShardedOrderTree<P>
where
    P: Price + Ord + PartialOrd + PartialEq + Eq + Clone,
{
    pub shards: Vec<RwLock<OrderTree<P>>>,
    pub shard_count: AtomicUsize,
}

impl<P> ShardedOrderTree<P>
where
    P: Price + Ord + PartialOrd + PartialEq + Eq + Clone,
{
    pub fn new(shard_count: usize) -> Self {
        let mut shards = Vec::with_capacity(shard_count);
        for _ in 0..shard_count {
            shards.push(RwLock::new(OrderTree {
                tree: BTreeMap::new(),
                total_orders: 0,
            }));
        }
        Self {
            shards,
            shard_count: AtomicUsize::new(shard_count),
        }
    }
    fn get_shard_index(&self, order_id: &str) -> usize {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        order_id.hash(&mut hasher);
        (hasher.finish() as usize) % self.shard_count.load(Ordering::SeqCst)
    }
    pub fn add_order(&self, order: Order) -> OrderLocation {
        let shard_index = self.get_shard_index(&order.id);
        let price_key = (order.price * 10000.0) as u64;

        {
            let mut shard = self.shards[shard_index].write();
            let price = P::new(order.price);
            shard
                .tree
                .entry(price)
                .or_insert_with(VecDeque::new)
                .push_back(order.clone());
            shard.total_orders += 1;
        }
        OrderLocation {
            price_key,
            direction: order.direction,
            shard_id: shard_index,
            order_id: order.id,
        }
    }
    pub fn get_best_price(&self) -> Option<P> {
        let mut best_price: Option<P> = None;
        for shard in &self.shards {
            let shard_guard = shard.read();
            let shard_best = if std::any::TypeId::of::<P>() == std::any::TypeId::of::<BidPrice>() {
                shard_guard.tree.keys().last().cloned()
            } else {
                shard_guard.tree.keys().next().cloned()
            };
            if let Some(price) = shard_best {
                match &best_price {
                    None => best_price = Some(price),
                    Some(current_best) => {
                        if (std::any::TypeId::of::<P>() == std::any::TypeId::of::<BidPrice>()
                            && price > *current_best)
                            || (std::any::TypeId::of::<P>() != std::any::TypeId::of::<BidPrice>()
                                && price < *current_best)
                        {
                            best_price = Some(price);
                        }
                    }
                }
            }
        }
        best_price
    }
    pub fn get_total_order_count(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.read().total_orders)
            .sum()
    }
    pub fn remove_order(&self, order_id: &str, shard_id: usize) -> bool {
        let mut shard = self.shards[shard_id].write();
        let operation_result = shard
            .tree
            .iter()
            .find_map(|(price, orders)| {
                orders
                    .iter()
                    .position(|order| order.id == order_id)
                    .map(|pos| (price.clone(), pos, orders.len()))
            })
            .and_then(|(price, index, original_len)| {
                shard.tree.get_mut(&price).map(|orders| {
                    orders.remove(index);
                    (price, original_len == 1)
                })
            });
        if let Some((price, is_empty)) = operation_result {
            shard.total_orders -= 1;
            if is_empty {
                shard.tree.remove(&price);
            }
            true
        } else {
            false
        }
    }
    /// Get the total order quantity at a specified price.
    pub fn get_order_count_by_price(&self, price: f64) -> usize {
        let price_key = P::new(price);
        let mut total_count = 0;

        for shard in &self.shards {
            let shard_guard = shard.read();
            if let Some(orders) = shard_guard.tree.get(&price_key) {
                total_count += orders.len();
            }
        }
        total_count
    }
    /// Get the total base quantity at the specified price.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │    INPUT PRICE   │   ORDER STATE    │  BASE TOKEN      │  RETURN VALUE    │
    /// │   (QUOTE)        │   AT PRICE LEVEL │  CALCULATION     │  (BASE TOTAL)    │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 50000.0 │ Shard0: [0.5 BTC,│ 0.5 + 1.2 + 0.3  │      2.0 BTC     │
    /// │     (USDT)       │ 1.2 BTC]         │                  │                  │
    /// │                  │ Shard1: [0.3 BTC]│                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 51000.0 │ All shards: []   │ 0.0              │      0.0 BTC     │
    /// │     (USDT)       │ (No orders)      │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 2.5     │ Shard0: [1000 ETH│ 1000 + 500       │     1500 ETH     │
    /// │     (USDT)       │ Shard1: [500 ETH]│                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 1.0     │ Shard2: [50000   │ 50000            │    50000 ADA     │
    /// │     (USDT)       │ ADA]             │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    pub fn get_total_base_unit_by_price(&self, price: f64) -> f64 {
        let price_key = P::new(price);
        let mut total_quantity = 0.0;
        for shard in &self.shards {
            let shard_guard = shard.read();
            if let Some(orders) = shard_guard.tree.get(&price_key) {
                for order in orders {
                    total_quantity += order.remaining;
                }
            }
        }
        total_quantity
    }

    /// Get the total value of the quote for the specified price.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │    INPUT PRICE   │   ORDER STATE    │  QUOTE TOKEN     │  RETURN VALUE    │
    /// │   (QUOTE)        │   AT PRICE LEVEL │  CALCULATION     │  (QUOTE TOTAL)   │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 50000.0 │ Shard0: [0.5 BTC,│ (0.5 × 50000) +  │   100000.0 USD   │
    /// │     (USDT)       │ 1.2 BTC]         │ (1.2 × 50000) +  │                  │
    /// │                  │ Shard1: [0.3 BTC]│ (0.3 × 50000)    │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 51000.0 │ All shards: []   │ 0.0              │      0.0 USD     │
    /// │     (USDT)       │ (No orders)      │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 2.5     │ Shard0: [1000 ETH│ (1000 × 2.5) +   │     3750.0 USD   │
    /// │     (USDT)       │ Shard1: [500 ETH]│ (500 × 2.5)      │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 1.0     │ Shard2: [50000   │ 50000 × 1.0      │   50000.0 USD    │
    /// │     (USDT)       │ ADA]             │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    pub fn get_total_quote_value_by_price(&self, price: f64) -> f64 {
        let price_key = P::new(price);
        let mut total_quote_value = 0.0;
        for shard in &self.shards {
            let shard_guard = shard.read();
            if let Some(orders) = shard_guard.tree.get(&price_key) {
                for order in orders {
                    // quote_value = base_quantity * price
                    total_quote_value += order.remaining * price;
                }
            }
        }
        total_quote_value
    }

    /// Get a map of all price tiers relative to order quantities.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │   PRICE LEVEL    │   ORDER STATE    │  ORDER COUNT     │  RETURN VALUE    │
    /// │    (Quote)       │   AT PRICE LEVEL │  CALCULATION     │  (Order Total)   │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50000.0         │ [OrderA, OrderB, │ 2 + 1 = 3        │        3         │
    /// │  (USDT/BTC)      │ OrderC]+[OrderD] │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50100.0         │ [OrderE]         │ 1                │        1         │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50200.0         │ [OrderF, OrderG] │ 2                │        2         │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  49900.0         │ [] (No orders)   │ 0                │        0         │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    /// # Returns
    /// * map { k: price level, v: order count }
    pub fn get_price_level_total_order(&self) -> BTreeMap<u64, usize> {
        let mut distribution = BTreeMap::new();
        for shard in &self.shards {
            let shard_guard = shard.read();
            for (price, orders) in &shard_guard.tree {
                let price_key = (price.to_f64() * 10000.0) as u64;
                *distribution.entry(price_key).or_insert(0) += orders.len();
            }
        }
        distribution
    }
    /// Gets a map of the sum of base units for all price levels.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │   PRICE LEVEL    │   ORDER STATE    │  BASE QUANTITY   │  RETURN VALUE    │
    /// │    (Quote)       │   AT PRICE LEVEL │  CALCULATION     │  (Base Total)    │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50000.0         │ [0.5 BTC,1.2 BTC │ 0.5 + 1.2 + 0.3  │      2.0 BTC     │
    /// │  (USDT/BTC)      │ ] + [0.3 BTC]    │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  2500.0          │ [1000 ETH,       │ 1000 + 500       │     1500 ETH     │
    /// │  (USDT/ETH)      │ 500 ETH]         │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  1.0             │ [50000 ADA]      │ 50000            │    50000 ADA     │
    /// │  (USDT/ADA)      │                  │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  51000.0         │ [] (No orders)   │ 0.0              │      0.0 BTC     │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    /// # Returns
    /// * map { k: price level, v: base unit total count }
    pub fn get_price_level_total_base_unit(&self) -> BTreeMap<u64, f64> {
        let mut distribution = BTreeMap::new();
        for shard in &self.shards {
            let shard_guard = shard.read();
            for (price, orders) in &shard_guard.tree {
                let price_key = (price.to_f64() * 10000.0) as u64;
                let total_quantity: f64 = orders.iter().map(|order| order.remaining).sum();
                *distribution.entry(price_key).or_insert(0.0) += total_quantity;
            }
        }
        distribution
    }

    /// Get the total value of quotes for all price levels.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │   PRICE LEVEL    │   ORDER STATE    │  QUOTE VALUE     │  RETURN VALUE    │
    /// │    (Quote)       │   AT PRICE LEVEL │  CALCULATION     │  (Quote Total)   │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50000.0         │ [0.5 BTC, 1.2 BTC│ (0.5×50000) +    │   100000 USD     │
    /// │  (USDT/BTC)      │ ] + [0.3 BTC]    │ (1.2×50000) +    │                  │
    /// │                  │                  │ (0.3×50000)      │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  2500.0          │ [1000 ETH,       │ (1000×2500) +    │   3750000 USD    │
    /// │  (USDT/ETH)      │ 500 ETH]         │ (500×2500)       │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  1.0             │ [50000 ADA]      │ 50000 × 1.0      │    50000 USD     │
    /// │  (USDT/ADA)      │                  │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  51000.0         │ [] (No orders)   │ 0.0              │      0 USD       │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    /// # Returns
    /// * map { k: price level, v: total quote value }
    pub fn get_price_level_total_quote_value(&self) -> BTreeMap<u64, f64> {
        let mut distribution = BTreeMap::new();
        for shard in &self.shards {
            let shard_guard = shard.read();
            for (price, orders) in &shard_guard.tree {
                let price_key = (price.to_f64() * 10000.0) as u64;
                let total_quote_value: f64 = orders
                    .iter()
                    .map(|order| order.remaining * price.to_f64())
                    .sum();
                *distribution.entry(price_key).or_insert(0.0) += total_quote_value;
            }
        }
        distribution
    }
}

#[derive(Debug)]
pub struct SlfeStats {
    pub orders_processed: u64,
    pub orders_matched: u64,
    pub total_quantity: f64,
    pub avg_match_latency_us: f64,
    pub peak_tps: u64,
    pub current_queue_depth: usize,
    pub last_update: Instant,
}

impl SlfeStats {
    fn new() -> Self {
        Self {
            orders_processed: 0,
            orders_matched: 0,
            total_quantity: 0.0,
            avg_match_latency_us: 0.0,
            peak_tps: 0,
            current_queue_depth: 0,
            last_update: Instant::now(),
        }
    }
}

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
/// * order_index: Fast concurrent mapping from order IDs to their locations in the books
/// * stats: Runtime statistics protected by read-write locks
/// * config: Engine configuration parameters
///
#[derive(Debug, Clone)]
pub struct Slfe {
    pub bids: Arc<ShardedOrderTree<BidPrice>>,
    pub asks: Arc<ShardedOrderTree<AskPrice>>,
    pub tx: Sender<MatchEvent>,
    pub rx: Arc<Receiver<MatchEvent>>,
    pub order_index: Arc<DashMap<String, OrderLocation>>,
    pub stats: Arc<RwLock<SlfeStats>>,
    pub config: Arc<RwLock<MatchEngineConfig>>,
}

impl Slfe {
    pub fn new(config: Option<MatchEngineConfig>) -> Self {
        let (tx, rx) = unbounded();
        let config = config.unwrap();
        Self {
            bids: Arc::new(ShardedOrderTree::new(config.shard_count)),
            asks: Arc::new(ShardedOrderTree::new(config.shard_count)),
            tx: tx,
            rx: Arc::new(rx),
            order_index: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(SlfeStats::new())),
            config: Arc::new(RwLock::new(config)),
        }
    }
    pub async fn start_engine(self: Arc<Self>) {
        let mut tasks = Vec::new();
        let event_matcher = self.clone();
        let event_handle = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(event_matcher.start_event_engine())
        });
        tasks.push(event_handle);
        let depth_matcher = self.clone();
        tasks.push(tokio::spawn(async move {
            depth_matcher.start_depth_monitor().await;
        }));
        for task in tasks {
            join!(task);
        }
    }
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
            let depth = s.clone().get_market_depth().await;
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
            if let Some(results) = self.execute_immediate_match().await {
                if !results.is_empty() {
                    match_occurred = true;
                    total_matched += results.len();
                    self.notify_match_results(results).await;
                }
            }
            if total_matched > 1000 {
                break;
            }
        }
    }
    async fn execute_immediate_match(&self) -> Option<Vec<MatchResult>> {
        let best_bid = self.bids.get_best_price();
        let best_ask = self.asks.get_best_price();
        if let (Some(bid_price), Some(ask_price)) = (best_bid, best_ask) {
            if bid_price.price >= ask_price.price {
                return Some(self.match_across_shards(&bid_price, &ask_price).await);
            }
        }
        None
    }
    async fn match_across_shards(
        &self,
        bid_price: &BidPrice,
        ask_price: &AskPrice,
    ) -> Vec<MatchResult> {
        let mut all_results = Vec::new();
        for bid_shard_id in 0..self.config.read().shard_count {
            for ask_shard_id in 0..self.config.read().shard_count {
                let results = self
                    .match_shard_pair(bid_shard_id, ask_shard_id, bid_price, ask_price)
                    .await;
                all_results.extend(results);
            }
        }
        all_results
    }
    async fn match_shard_pair(
        &self,
        bid_shard_id: usize,
        ask_shard_id: usize,
        bid_price: &BidPrice,
        ask_price: &AskPrice,
    ) -> Vec<MatchResult> {
        let (bid_orders_data, ask_orders_data) = {
            let bid_shard = self.bids.shards[bid_shard_id].read();
            let ask_shard = self.asks.shards[ask_shard_id].read();
            let bid_orders = bid_shard.tree.get(bid_price).cloned();
            let ask_orders = ask_shard.tree.get(ask_price).cloned();
            (bid_orders, ask_orders)
        };
        let (mut bid_orders, mut ask_orders) = match (bid_orders_data, ask_orders_data) {
            (Some(bid), Some(ask)) => (bid, ask),
            _ => return Vec::new(),
        };
        let original_bid_count = bid_orders.len();
        let original_ask_count = ask_orders.len();
        let results = self
            .match_order_queues(&mut bid_orders, &mut ask_orders)
            .await;
        if results.is_empty() {
            return results;
        }
        {
            let mut bid_shard = self.bids.shards[bid_shard_id].write();
            let mut ask_shard = self.asks.shards[ask_shard_id].write();
            let matched_bid_orders = original_bid_count - bid_orders.len();
            let matched_ask_orders = original_ask_count - ask_orders.len();
            bid_shard.total_orders -= matched_bid_orders;
            ask_shard.total_orders -= matched_ask_orders;
            if bid_orders.is_empty() {
                bid_shard.tree.remove(bid_price);
            } else {
                bid_shard.tree.insert(bid_price.clone(), bid_orders);
            }
            if ask_orders.is_empty() {
                ask_shard.tree.remove(ask_price);
            } else {
                ask_shard.tree.insert(ask_price.clone(), ask_orders);
            }
        }
        results
    }
    async fn match_order_queues(
        &self,
        bid_orders: &mut VecDeque<Order>,
        ask_orders: &mut VecDeque<Order>,
    ) -> Vec<MatchResult> {
        let mut results = Vec::new();
        let match_price = bid_orders
            .front()
            .unwrap()
            .price
            .min(ask_orders.front().unwrap().price);
        let mut bid_index = 0;
        let mut ask_index = 0;
        while bid_index < bid_orders.len() && ask_index < ask_orders.len() {
            let bid_order = &mut bid_orders[bid_index];
            let ask_order = &mut ask_orders[ask_index];
            if bid_order.can_trade() && ask_order.can_trade() {
                let match_qty = bid_order.remaining.min(ask_order.remaining);
                if match_qty > 0.0 {
                    results.push(MatchResult {
                        bid_order_id: bid_order.id.clone(),
                        ask_order_id: ask_order.id.clone(),
                        price: match_price,
                        quantity: match_qty,
                        timestamp: Instant::now(),
                    });
                    bid_order.execute_trade(match_qty);
                    ask_order.execute_trade(match_qty);
                }
                if bid_order.remaining <= 0.0 {
                    bid_orders.remove(bid_index);
                } else {
                    bid_index += 1;
                }
                if ask_order.remaining <= 0.0 {
                    ask_orders.remove(ask_index);
                } else {
                    ask_index += 1;
                }
            } else {
                if !bid_order.can_trade() {
                    bid_index += 1;
                }
                if !ask_order.can_trade() {
                    ask_index += 1;
                }
            }
        }
        results
    }
    async fn notify_match_results(&self, results: Vec<MatchResult>) {
        for result in results {
            if result.quantity > 0.0 {
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
    async fn handle_cancellation(self: Arc<Self>, order_id: &str) {
        if let Some(location) = self.order_index.get(order_id) {
            match location.direction {
                OrderDirection::Buy => {
                    self.bids.remove_order(order_id, location.shard_id);
                }
                OrderDirection::Sell => {
                    self.asks.remove_order(order_id, location.shard_id);
                }
                OrderDirection::None => (),
            }
            self.order_index.remove(order_id);
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
                MatchEvent::NewOrder(_order) => {
                    processed += 1;
                    let self_clone = self.clone();
                    if let Some(results) = self_clone.execute_immediate_match().await {
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
                    if let Some(results) = self.execute_immediate_match().await {
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
    fn update_stats(&self, processed: usize, matched: usize, quantity: f64, latency: Duration) {
        let mut stats = self.stats.write();
        stats.orders_processed += processed as u64;
        stats.orders_matched += matched as u64;
        stats.total_quantity += quantity;
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
    async fn get_market_depth(self: Arc<Self>) -> MarketDepth {
        MarketDepth {
            bid_order_count: self.bids.get_total_order_count(),
            ask_order_count: self.asks.get_total_order_count(),
            best_bid: self.bids.get_best_price().map(|p| p.to_f64()),
            best_ask: self.asks.get_best_price().map(|p| p.to_f64()),
            spread: self.calculate_spread(),
            timestamp: Instant::now(),
        }
    }
    fn calculate_spread(self: Arc<Self>) -> f64 {
        if let (Some(best_bid), Some(best_ask)) =
            (self.bids.get_best_price(), self.asks.get_best_price())
        {
            best_ask.to_f64() - best_bid.to_f64()
        } else {
            0.0
        }
    }
    /// add order
    pub fn add_order(&self, order: Order) -> Result<(), MatchEngineError> {
        if order.price <= 0.0 {
            return Err(MatchEngineError::AddOrderError(
                "Price must be greater than 0".to_string(),
            ));
        }
        if order.quantity <= 0.0 {
            return Err(MatchEngineError::AddOrderError(
                "The quantity must be greater than 0".to_string(),
            ));
        }
        if order.id.is_empty() {
            return Err(MatchEngineError::AddOrderError(
                "Order ID cannot be empty".to_string(),
            ));
        }
        let start_time = Instant::now();
        let location = match order.direction {
            OrderDirection::Buy => self.bids.add_order(order.clone()),
            OrderDirection::Sell => self.asks.add_order(order.clone()),
            OrderDirection::None => {
                return Err(MatchEngineError::AddOrderError(format!(
                    "Order Direction ERROR"
                )));
            }
        };
        self.order_index.insert(order.id.clone(), location);
        if let Err(e) = self.tx.send(MatchEvent::NewOrder(order)) {
            return Err(MatchEngineError::AddOrderError(format!(
                "Event queue full: {}",
                e
            )));
        }
        let self_clone = self.clone();
        self_clone.update_stats(1, 0, 0.0, start_time.elapsed());
        Ok(())
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
}
