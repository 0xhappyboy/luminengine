use atomic_plus::AtomicF64;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Order direction enumeration.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderDirection {
    Buy,
    Sell,
    None,
}

impl OrderDirection {
    /// Converts a string to an OrderDirection.
    pub fn from_string(s: String) -> OrderDirection {
        if s == "Buy".to_string() {
            OrderDirection::Buy
        } else if s == "Sell".to_string() {
            OrderDirection::Sell
        } else {
            OrderDirection::None
        }
    }
}

/// Order status enumeration.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    /// Waiting for deal.
    Pending,
    /// Order partial deal.
    Partial,
    /// Order complete deal.
    Filled,
    /// Order has been canceled.
    Cancelled,
    /// The order expiration date.
    Expired,
}

impl Default for OrderStatus {
    fn default() -> Self {
        OrderStatus::Pending
    }
}

/// Order type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Hash)]
pub enum OrderType {
    Limit,
    Market,
    Stop,
    StopLimit,
    FOK,
    IOC,
    Iceberg,
    DAY,
    GTC,
}

/// Optimized order structure (supports high-performance operations).
#[derive(Debug)]
pub struct Order {
    // Atomic fields (support concurrent updates).
    /// Unique order identifier.
    pub id: String,
    /// Trading symbol.
    pub symbol: String,
    /// Order price (atomic).
    pub price: AtomicF64,
    /// Order direction (buy, sell, none).
    pub direction: OrderDirection,
    /// Total order quantity (atomic).
    pub quantity: AtomicF64,
    /// Remaining quantity (atomic).
    pub remaining: AtomicF64,
    /// Filled quantity (atomic).
    pub filled: AtomicF64,
    /// Creation time string.
    pub crt_time: String,
    /// Order status (atomic wrapper).
    pub status: AtomicOrderStatus,
    /// Expiration instant, if any.
    pub expiry: Option<Instant>,
    /// Order type.
    pub order_type: OrderType,
    /// Exchange identifier, if any.
    pub ex: Option<String>,
    /// Version number (for optimistic locking).
    pub version: AtomicU64,
    /// Timestamp in nanoseconds (for time priority).
    pub timestamp_ns: u64,
    /// Parent order ID (for iceberg orders etc.).
    pub parent_order_id: Option<String>,
    /// Order priority.
    pub priority: u32,
}

/// Atomic order status wrapper.
#[derive(Debug)]
pub struct AtomicOrderStatus {
    inner: AtomicU64,
}

impl AtomicOrderStatus {
    /// Creates a new atomic order status.
    pub fn new(status: OrderStatus) -> Self {
        Self {
            inner: AtomicU64::new(status as u64),
        }
    }

    /// Loads the current order status.
    pub fn load(&self, ordering: Ordering) -> OrderStatus {
        match self.inner.load(ordering) {
            0 => OrderStatus::Pending,
            1 => OrderStatus::Partial,
            2 => OrderStatus::Filled,
            3 => OrderStatus::Cancelled,
            4 => OrderStatus::Expired,
            _ => OrderStatus::Pending,
        }
    }

    /// Stores a new order status.
    pub fn store(&self, status: OrderStatus, ordering: Ordering) {
        self.inner.store(status as u64, ordering);
    }

    /// Atomically compares and exchanges the order status.
    pub fn compare_exchange(
        &self,
        current: OrderStatus,
        new: OrderStatus,
        success: Ordering,
        failure: Ordering,
    ) -> Result<OrderStatus, OrderStatus> {
        match self
            .inner
            .compare_exchange(current as u64, new as u64, success, failure)
        {
            Ok(val) => match val {
                0 => Ok(OrderStatus::Pending),
                1 => Ok(OrderStatus::Partial),
                2 => Ok(OrderStatus::Filled),
                3 => Ok(OrderStatus::Cancelled),
                4 => Ok(OrderStatus::Expired),
                _ => Ok(OrderStatus::Pending),
            },
            Err(val) => match val {
                0 => Err(OrderStatus::Pending),
                1 => Err(OrderStatus::Partial),
                2 => Err(OrderStatus::Filled),
                3 => Err(OrderStatus::Cancelled),
                4 => Err(OrderStatus::Expired),
                _ => Err(OrderStatus::Pending),
            },
        }
    }
}
