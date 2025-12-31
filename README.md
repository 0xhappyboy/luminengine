<h1 align="center">
    ✨ Luminengine
</h1>
<h4 align="center">
High-performance financial order book matching engine.
</h4>
<p align="center">
  <a href="https://github.com/0xhappyboy/luminengine/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-AGPL3.0-d1d1f6.svg?style=flat&labelColor=1C2C2E&color=BEC5C9&logo=googledocs&label=license&logoColor=BEC5C9" alt="License"></a>
</p>
<p align="center">
<a href="./README_zh-CN.md">简体中文</a> | <a href="./README.md">English</a>
</p>

## 📚 Directory

| **directory**        | **role**                             |
| :------------------- | :----------------------------------- |
| **core**             | Order book engine core code.         |
| **manager-desktop**  | Order Book Engine Desktop Manager.   |
| **emulator-desktop** | Desktop order book engine simulator. |
| **server**           | Engine service command line program. |
| **terminal**         | Command line terminal visualizer     |
| **builder/rpc**      | RPC protocol file builder.           |

## Features

- **Fully Lock-Free Design**: Based on Crossbeam Epoch's lock-free data structures and atomic operations
- **Multi-threaded Concurrency**: Independent order processor and matching engine threads
- **Multiple Order Type Support**:
  - Limit Orders
  - Market Orders
  - Stop Orders
  - Stop-Limit Orders
  - Immediate or Cancel (IOC/FOK)
  - Iceberg Orders
  - DAY Orders (Valid Today)
  - GTC Orders (Good Till Cancelled)
- **High Performance**: Skip-list order tree + lock-free queues
- **Real-time Statistics**: Order book depth, matching statistics, queue status monitoring

## Architecture Overview

```
OrderBook
├── OrderTree (Bids) - Skip-list based lock-free buy side order tree
├── OrderTree (Asks) - Skip-list based lock-free sell side order tree
├── OrderQueue - Lock-free order queue at each price level
├── MatchEngine - Multi-order type matching engine
│   ├── LimitOrderHandler - Limit order handler
│   ├── MarketOrderHandler - Market order handler
│   ├── StopOrderHandler - Stop order handler
│   ├── StopLimitOrderHandler - Stop-limit order handler
│   ├── ImmediateOrderHandler - IOC/FOK order handler
│   ├── IcebergOrderHandler - Iceberg order handler
│   ├── DayOrderHandler - DAY order handler
│   └── GTCOrderHandler - GTC order handler
├── order_processor_threads - Order processing thread pool
└── matching_threads - Matching execution threads
```

## Quick Start

### Create Order Book

```rust
use orderbook::OrderBook;
use std::sync::Arc;

fn main() {
// Create new order book
let orderbook = Arc::new(OrderBook::new("BTC/USDT"));

    // Set matching callback
    orderbook.set_match_callback(|match_result| {
        println!("Match executed: {:?}", match_result);
    });

}
```

### Add Orders

```rust
use orderbook::order::{Order, OrderDirection, OrderType};
use atomic_plus::AtomicF64;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use crate::orderbook::order::{AtomicOrderStatus, OrderStatus};

fn create_order() -> Arc<Order> {
Arc::new(Order {
id: "order_001".to_string(),
symbol: "BTC/USDT".to_string(),
price: AtomicF64::new(50000.0),
direction: OrderDirection::Buy,
quantity: AtomicF64::new(1.0),
remaining: AtomicF64::new(1.0),
filled: AtomicF64::new(0.0),
crt_time: chrono::Utc::now().to_rfc3339(),
status: AtomicOrderStatus::new(OrderStatus::Pending),
expiry: None,
order_type: OrderType::Limit,
ex: Some("exchange".to_string()),
version: AtomicU64::new(1),
timestamp_ns: 0,
parent_order_id: None,
priority: 0,
})
}

// Add single order
let order = create_order();
orderbook.add_order_async(order.clone())?;

// Batch add orders
let orders = vec![order1, order2, order3];
orderbook.add_orders_batch(orders)?;
```

### Query Order Book Status

```rust
// Get market depth
let depth = orderbook.get_market_depth(10);
println!("Bids: {:?}", depth.bids);
println!("Asks: {:?}", depth.asks);

// Get statistics
let stats = orderbook.get_stats();
println!("Total orders: {}, Active orders: {}", stats.0, stats.1);

// Get queue status
let queue_status = orderbook.get_queue_status();
println!("Channel size: {}, Processors: {}, Matchers: {}",
queue_status.0, queue_status.1, queue_status.2);

// Find order
if let Some(order) = orderbook.find_order("order_001") {
println!("Found order: {:?}", order);
}
```

### Different Order Type Examples

```rust
// Market order
let market_order = Order {
order_type: OrderType::Market,
// ... other fields
};

// Iceberg order
let iceberg_order = Order {
order_type: OrderType::Iceberg,
quantity: AtomicF64::new(1000.0), // Total quantity
// Engine automatically splits into smaller orders
};

// Stop order
let stop_order = Order {
order_type: OrderType::Stop,
price: AtomicF64::new(45000.0), // Trigger price
direction: OrderDirection::Sell,
// Triggers when price <= 45000
};

// IOC order (Immediate or Cancel)
let ioc_order = Order {
order_type: OrderType::IOC,
// Cancels if cannot be executed immediately
};
```

### Shutdown Order Book

```rust
// Manual shutdown
orderbook.shutdown();

// Or rely on Drop trait for automatic shutdown
drop(orderbook);
```

## Performance Characteristics

- **Lock-Free Design**: Eliminates lock contention, improves multi-core CPU utilization
- **Memory Safe**: Based on Rust's ownership system, avoids data races
- **Real-time**: Microsecond-level matching latency
- **Scalable**: Automatically adjusts thread count based on CPU cores

## Core API

- `OrderBook::new(symbol: &str)` - Create order book
- `add_order_async(order: Arc<Order>)` - Add order asynchronously
- `add_orders_batch(orders: Vec<Arc<Order>>)` - Batch add orders
- `get_market_depth(levels: usize)` - Get market depth
- `find_order(order_id: &str)` - Find order by ID
- `get_stats()` - Get statistics
- `get_queue_status()` - Get queue status
- `shutdown()` - Shutdown order book
