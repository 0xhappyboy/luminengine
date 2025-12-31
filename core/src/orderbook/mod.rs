pub mod order;
use crate::matcher::{MatchEngine, MatchResult, MatchStats};
use crate::orderbook::order::{Order, OrderDirection};
use crossbeam::channel;
use crossbeam::epoch::{self, Atomic, Guard, Owned};
use num_cpus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const MAX_ORDER_PROCESSOR_THREADS: usize = 5;

/// Order book statistics.
#[derive(Debug)]
pub struct OrderBookStats {
    /// Total number of orders.
    pub total_orders: AtomicUsize,
    /// Number of active orders.
    pub active_orders: AtomicUsize,
    /// Number of add operations.
    pub add_operations: AtomicU64,
}

impl OrderBookStats {
    /// Creates new statistics.
    pub fn new() -> Self {
        OrderBookStats {
            total_orders: AtomicUsize::new(0),
            active_orders: AtomicUsize::new(0),
            add_operations: AtomicU64::new(0),
        }
    }
}

/// A completely lock-free order book core structure.
#[derive(Debug)]
pub struct OrderBook {
    /// Symbol identifier for the order book.
    pub symbol: Arc<str>,
    /// Buy-side order tree.
    pub bids: Arc<OrderTree>,
    /// Sell-side order tree.
    pub asks: Arc<OrderTree>,
    /// Statistics for the order book.
    pub stats: Arc<OrderBookStats>,
    match_engine: Arc<MatchEngine>,
    pub stopped: Arc<AtomicBool>,
    pub matching_thread: parking_lot::Mutex<Option<thread::JoinHandle<()>>>,
    order_channel: channel::Sender<(Arc<Order>, u64, bool)>,
    order_processor_threads: parking_lot::Mutex<Vec<thread::JoinHandle<()>>>,
}

impl OrderBook {
    /// Creates a new order book for the given symbol with built-in matching engine.
    pub fn new(symbol: &str) -> Self {
        let symbol_arc: Arc<str> = Arc::from(symbol);
        let stopped = Arc::new(AtomicBool::new(false));
        let (order_sender, order_receiver) = channel::unbounded();
        let orderbook = OrderBook {
            symbol: symbol_arc.clone(),
            bids: Arc::new(OrderTree::new(true)),
            asks: Arc::new(OrderTree::new(false)),
            stats: Arc::new(OrderBookStats::new()),
            match_engine: Arc::new(MatchEngine::new()),
            stopped: stopped.clone(),
            matching_thread: parking_lot::Mutex::new(None),
            order_channel: order_sender,
            order_processor_threads: parking_lot::Mutex::new(Vec::new()),
        };
        orderbook.start_order_processors(order_receiver);
        orderbook.start_matching_engines();
        orderbook
    }

    /// Start multiple order processor threads
    fn start_order_processors(&self, receiver: channel::Receiver<(Arc<Order>, u64, bool)>) {
        let num_processors = num_cpus::get().max(MAX_ORDER_PROCESSOR_THREADS);
        for i in 0..num_processors {
            let receiver_clone = receiver.clone();
            let bids = self.bids.clone();
            let asks = self.asks.clone();
            let match_engine = self.match_engine.clone();
            let stats = self.stats.clone();
            let stopped = self.stopped.clone();
            let handle = thread::spawn(move || {
                let mut processed_count = 0;
                while !stopped.load(Ordering::Relaxed) {
                    match receiver_clone.recv_timeout(Duration::from_millis(100)) {
                        Ok((order, price, is_bid)) => {
                            Self::process_single_order_sync(
                                &bids,
                                &asks,
                                &match_engine,
                                &stats,
                                order,
                                price,
                                is_bid,
                            );
                            processed_count += 1;
                        }
                        Err(channel::RecvTimeoutError::Timeout) => {
                            continue;
                        }
                        Err(channel::RecvTimeoutError::Disconnected) => {
                            break;
                        }
                    }
                }
            });
            self.order_processor_threads.lock().push(handle);
        }
    }

    /// Synchronous processing of a single order (internal method)
    fn process_single_order_sync(
        bids: &OrderTree,
        asks: &OrderTree,
        match_engine: &MatchEngine,
        stats: &OrderBookStats,
        order: Arc<Order>,
        price: u64,
        is_bid: bool,
    ) {
        // Validate order
        let quantity = order.quantity.load(Ordering::Relaxed);
        if quantity <= 0.0 {
            return;
        }
        match order.direction {
            OrderDirection::Buy => {
                // Add to buy tree
                bids.add_order(order.clone(), price);
                // Add to matching engine
                if let Err(e) = match_engine.add_order(order.clone(), price, true) {
                    eprintln!("Failed to add buy order to match engine: {}", e);
                }
            }
            OrderDirection::Sell => {
                // Add to sell tree
                asks.add_order(order.clone(), price);
                // Add to matching engine
                if let Err(e) = match_engine.add_order(order.clone(), price, false) {
                    eprintln!("Failed to add sell order to match engine: {}", e);
                }
            }
            OrderDirection::None => {
                return;
            }
        }
        // Update statistics
        stats.total_orders.fetch_add(1, Ordering::Relaxed);
        stats.active_orders.fetch_add(1, Ordering::Relaxed);
        stats.add_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Start multiple matching engine threads
    fn start_matching_engines(&self) {
        let match_engine = self.match_engine.clone();
        let bids = self.bids.clone();
        let asks = self.asks.clone();
        let symbol = self.symbol.clone();
        let stopped = self.stopped.clone();
        let match_engine_clone = match_engine.clone();
        let bids_clone = bids.clone();
        let asks_clone = asks.clone();
        let symbol_clone = symbol.clone();
        let stopped_clone = stopped.clone();
        let handle = thread::spawn(move || {
            while !stopped_clone.load(Ordering::Relaxed) {
                // Execute matching cycle
                match_engine_clone.execute_price_discovery(&bids_clone, &asks_clone, &symbol_clone);
            }
        });
        *self.matching_thread.lock() = Some(handle);
    }

    /// Asynchronously adds a new order to the order book.
    pub fn add_order_async(&self, order: Arc<Order>) -> Result<(), String> {
        // Fast validation
        let quantity = order.quantity.load(Ordering::Relaxed);
        if quantity <= 0.0 {
            return Err("Invalid order quantity".to_string());
        }
        let price = order.price.load(Ordering::Relaxed) as u64;
        let is_bid = order.direction == OrderDirection::Buy;
        // Send to async queue
        self.order_channel
            .send((order, price, is_bid))
            .map_err(|_| "Failed to send order to processing queue".to_string())?;
        Ok(())
    }

    /// Adds a new order to the order book (synchronous, for backward compatibility).
    pub fn add_order(&self, order: Arc<Order>) -> Result<(), String> {
        self.add_order_async(order)
    }

    /// Batch add orders
    pub fn add_orders_batch(&self, orders: Vec<Arc<Order>>) -> Result<(), String> {
        for order in orders {
            self.add_order_async(order)?;
        }
        Ok(())
    }

    pub fn set_match_callback<F>(&self, callback: F)
    where
        F: Fn(MatchResult) + Send + Sync + 'static,
    {
        self.match_engine.set_match_callback(callback);
    }

    pub fn shutdown(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.match_engine.stop();
        let mut processor_handles = self.order_processor_threads.lock();
        for (i, handle) in processor_handles.drain(..).enumerate() {
            if let Err(e) = handle.join() {
                eprintln!("Error joining processor thread {}: {:?}", i, e);
            }
        }
        let mut matcher_handle = self.matching_thread.lock();
        if let Some(handle) = matcher_handle.take() {
            if let Err(e) = handle.join() {
                eprintln!("Error joining matcher thread: {:?}", e);
            }
        }
    }
}

impl Drop for OrderBook {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl OrderBook {
    /// Searches for an order by its ID.
    pub fn find_order(&self, order_id: &str) -> Option<Arc<Order>> {
        // Search in both buy and sell trees.
        if let Some(order) = self.bids.find_order(order_id) {
            return Some(order);
        }
        if let Some(order) = self.asks.find_order(order_id) {
            return Some(order);
        }
        None
    }

    /// Retrieves order book statistics.
    pub fn get_stats(&self) -> (usize, usize, usize) {
        let total_orders = self.stats.total_orders.load(Ordering::Relaxed);
        let active_orders = self.stats.active_orders.load(Ordering::Relaxed);
        let bid_count = self.bids.size.load(Ordering::Relaxed);
        let ask_count = self.asks.size.load(Ordering::Relaxed);
        (total_orders, active_orders, bid_count + ask_count)
    }

    /// Retrieves a market depth snapshot for a given number of levels.
    pub fn get_market_depth(&self, levels: usize) -> MarketDepth {
        let bids = self.bids.get_price_levels(levels);
        let asks = self.asks.get_price_levels(levels);
        MarketDepth { bids, asks, levels }
    }

    pub fn get_match_stats(&self) -> Arc<MatchStats> {
        Arc::new(self.match_engine.get_stats())
    }

    /// Check if order book is running
    pub fn is_running(&self) -> bool {
        !self.stopped.load(Ordering::Relaxed)
    }

    /// Get queue status (for monitoring)
    pub fn get_queue_status(&self) -> (usize, usize, usize) {
        let channel_size = self.order_channel.len();
        let processor_count = self.order_processor_threads.lock().len();
        let matcher_count = 1;
        (channel_size, processor_count, matcher_count)
    }
}

/// Market depth snapshot.
#[derive(Debug, Clone)]
pub struct MarketDepth {
    /// Buy-side depth.
    pub bids: Vec<PriceLevelData>,
    /// Sell-side depth.
    pub asks: Vec<PriceLevelData>,
    /// Number of depth levels.
    pub levels: usize,
}

/// Price level data.
#[derive(Debug, Clone)]
pub struct PriceLevelData {
    pub price: u64,
    pub quantity: u64,
    pub order_count: usize,
}

/// Lock-free order tree (based on skip list).
#[derive(Debug)]
pub struct OrderTree {
    /// Head node of the skip list.
    pub head: Atomic<Node>,
    /// Maximum level of the skip list.
    pub max_level: usize,
    /// Number of orders in the tree.
    pub size: AtomicUsize,
    /// Whether this tree is for buy orders.
    pub is_bid: bool,
}

impl OrderTree {
    /// Creates a new order tree.
    pub fn new(is_bid: bool) -> Self {
        let max_level = 4; // Simplified: use 4-level skip list.
        let head = Node::new_head(max_level);
        OrderTree {
            head: Atomic::new(*head),
            max_level,
            size: AtomicUsize::new(0),
            is_bid,
        }
    }

    /// Adds an order to the order tree.
    pub fn add_order(&self, order: Arc<Order>, price: u64) {
        let guard = &epoch::pin();
        // Find or create the price node.
        let price_node = self.find_or_create_price_node(price, guard);
        // Create order queue if it does not exist.
        unsafe {
            if price_node.orders.load(Ordering::Relaxed, guard).is_null() {
                let order_queue = Owned::new(OrderQueue::new()).into_shared(guard);
                price_node.orders.store(order_queue, Ordering::Relaxed);
            }
            let order_queue = price_node
                .orders
                .load(Ordering::Relaxed, guard)
                .as_ref()
                .unwrap();
            order_queue.add_order(order.clone(), guard);
        }
        // Update statistics.
        self.size.fetch_add(1, Ordering::Relaxed);
    }

    /// Searches for an order by its ID within the tree.
    pub fn find_order(&self, order_id: &str) -> Option<Arc<Order>> {
        let guard = &epoch::pin();
        // Traverse all price nodes to find the order.
        let mut current = self.head.load(Ordering::Relaxed, guard);
        while let Some(node) = unsafe { current.as_ref() } {
            if node.price > 0 {
                // Skip the head node.
                if let Some(order_queue) =
                    unsafe { node.orders.load(Ordering::Relaxed, guard).as_ref() }
                {
                    if let Some(order) = order_queue.find_order(order_id, guard) {
                        return Some(order);
                    }
                }
            }
            current = node.forward[0].load(Ordering::Relaxed, guard);
        }
        None
    }

    /// Retrieves price level data for a specified number of levels.
    pub fn get_price_levels(&self, levels: usize) -> Vec<PriceLevelData> {
        let guard = &epoch::pin();
        let mut result = Vec::with_capacity(levels);
        let mut count = 0;
        let mut current = self.head.load(Ordering::Relaxed, guard);
        while let Some(node) = unsafe { current.as_ref() } {
            if node.price > 0 && count < levels {
                // Skip the head node.
                if let Some(order_queue) =
                    unsafe { node.orders.load(Ordering::Relaxed, guard).as_ref() }
                {
                    let price = node.price;
                    let quantity = order_queue.total_quantity.load(Ordering::Relaxed);
                    let order_count = order_queue.length.load(Ordering::Relaxed);

                    result.push(PriceLevelData {
                        price,
                        quantity,
                        order_count,
                    });
                    count += 1;
                }
            }
            current = node.forward[0].load(Ordering::Relaxed, guard);
        }
        // Sort based on buy or sell side.
        if self.is_bid {
            result.sort_by(|a, b| b.price.cmp(&a.price)); // Buy side: descending price.
        } else {
            result.sort_by(|a, b| a.price.cmp(&b.price)); // Sell side: ascending price.
        }
        result.truncate(levels);
        result
    }

    /// Finds or creates a price node (simplified implementation).
    fn find_or_create_price_node<'a>(&'a self, price: u64, guard: &'a Guard) -> &'a Node {
        let mut current = self.head.load(Ordering::Relaxed, guard);
        let mut prev = current;
        // Traverse the first level to find insertion point.
        while let Some(node) = unsafe { current.as_ref() } {
            let next = node.forward[0].load(Ordering::Relaxed, guard);
            if let Some(next_node) = unsafe { next.as_ref() } {
                if next_node.price == price {
                    // Found existing node.
                    return next_node;
                }
                // Check if we have reached the insertion point.
                let should_insert = if self.is_bid {
                    // Buy side: descending price, find first price <= insertion price.
                    next_node.price <= price
                } else {
                    // Sell side: ascending price, find first price >= insertion price.
                    next_node.price >= price
                };
                if should_insert {
                    break;
                }
                prev = current;
                current = next;
            } else {
                // Reached the end of the list.
                break;
            }
        }
        // Create new node (simplified: only insert into first level).
        let new_node = Owned::new(Node {
            price,
            orders: Atomic::null(),
            forward: {
                let mut vec = Vec::with_capacity(self.max_level);
                for _ in 0..self.max_level {
                    vec.push(Atomic::null());
                }
                vec
            },
            marked: AtomicBool::new(false),
            fully_linked: AtomicBool::new(true),
        })
        .into_shared(guard);
        // Insert the new node.
        unsafe {
            let prev_node = prev.as_ref().unwrap();
            let new_node_ref = new_node.as_ref().unwrap();
            // Update the forward pointer of the previous node.
            let next = prev_node.forward[0].load(Ordering::Relaxed, guard);
            new_node_ref.forward[0].store(next, Ordering::Relaxed);
            prev_node.forward[0].store(new_node, Ordering::Relaxed);
        }
        unsafe { new_node.as_ref().unwrap() }
    }
}

/// Skip list node.
#[derive(Debug)]
pub struct Node {
    /// Price key.
    pub price: u64,
    /// All orders at this price level.
    pub orders: Atomic<OrderQueue>,
    /// Forward pointer array.
    pub forward: Vec<Atomic<Node>>,
    /// Node marked flag.
    pub marked: AtomicBool,
    /// Node fully linked flag.
    pub fully_linked: AtomicBool,
}

impl Node {
    /// Creates a new head node.
    fn new_head(level: usize) -> Box<Self> {
        let mut forward = Vec::with_capacity(level);
        for _ in 0..level {
            forward.push(Atomic::null());
        }
        Box::new(Node {
            price: 0, // Head node price is 0.
            orders: Atomic::null(),
            forward,
            marked: AtomicBool::new(false),
            fully_linked: AtomicBool::new(true),
        })
    }
}

/// Lock-free order queue.
#[derive(Debug)]
pub struct OrderQueue {
    /// Head of the queue.
    pub head: Atomic<OrderNode>,
    /// Tail of the queue.
    pub tail: Atomic<OrderNode>,
    /// Length of the queue.
    pub length: AtomicUsize,
    /// Total quantity in the queue.
    pub total_quantity: AtomicU64,
}

impl OrderQueue {
    /// Creates a new order queue.
    fn new() -> Self {
        OrderQueue {
            head: Atomic::null(),
            tail: Atomic::null(),
            length: AtomicUsize::new(0),
            total_quantity: AtomicU64::new(0),
        }
    }

    /// Adds an order to the queue.
    fn add_order(&self, order: Arc<Order>, guard: &Guard) {
        let order_node = Owned::new(OrderNode {
            order: order.clone(),
            next: Atomic::null(),
        })
        .into_shared(guard);
        // Add to the tail of the queue.
        let tail = self.tail.load(Ordering::Relaxed, guard);
        if let Some(tail_node) = unsafe { tail.as_ref() } {
            tail_node.next.store(order_node, Ordering::Relaxed);
        } else {
            // Queue is empty, set as head.
            self.head.store(order_node, Ordering::Relaxed);
        }
        self.tail.store(order_node, Ordering::Relaxed);
        self.length.fetch_add(1, Ordering::Relaxed);
        self.total_quantity.fetch_add(
            order.quantity.load(Ordering::Relaxed) as u64,
            Ordering::Relaxed,
        );
    }

    /// Searches for an order by ID within the queue.
    fn find_order(&self, order_id: &str, guard: &Guard) -> Option<Arc<Order>> {
        let mut current = self.head.load(Ordering::Relaxed, guard);
        while let Some(node) = unsafe { current.as_ref() } {
            if node.order.id == order_id {
                return Some(node.order.clone());
            }
            current = node.next.load(Ordering::Relaxed, guard);
        }
        None
    }
}

/// Order node in the queue.
#[derive(Debug)]
pub struct OrderNode {
    /// Reference to the order.
    pub order: Arc<Order>,
    /// Next order node.
    pub next: Atomic<OrderNode>,
}
