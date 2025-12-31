<h1 align="center">
    ✨ Luminengine
</h1>
<h4 align="center">
高性能金融订单簿撮合引擎.
</h4>
<p align="center">
  <a href="https://github.com/0xhappyboy/luminengine/blob/main/core/LICENSE"><img src="https://img.shields.io/badge/License-AGPL3.0-d1d1f6.svg?style=flat&labelColor=1C2C2E&color=BEC5C9&logo=googledocs&label=license&logoColor=BEC5C9" alt="License"></a>
  <a href="https://crates.io/crates/luminengine">
<img src="https://img.shields.io/badge/crates-luminengine-20B2AA.svg?style=flat&labelColor=0F1F2D&color=FFD700&logo=rust&logoColor=FFD700">
</a>
</p>
<p align="center">
<a href="./README_zh-CN.md">简体中文</a> | <a href="./README.md">English</a>
</p>

## 特性

- **完全无锁设计**：基于 Crossbeam Epoch 的无锁数据结构和原子操作
- **多线程并发**：独立的订单处理器和撮合引擎线程
- **多订单类型支持**：
  - 限价单 (Limit)
  - 市价单 (Market)
  - 止损单 (Stop)
  - 止损限价单 (Stop-Limit)
  - 立即或取消 (IOC/FOK)
  - 冰山订单 (Iceberg)
  - 当日有效 (DAY)
  - 取消前有效 (GTC)
- **高性能**：跳表订单树 + 无锁队列
- **实时统计**：订单簿深度、撮合统计、队列状态监控

## 架构概览

```
OrderBook
├── OrderTree (Bids) - 跳表实现的无锁买盘订单树
├── OrderTree (Asks) - 跳表实现的无锁卖盘订单树
├── OrderQueue - 每个价格级别的无锁订单队列
├── MatchEngine - 多订单类型撮合引擎
│   ├── LimitOrderHandler - 限价单处理器
│   ├── MarketOrderHandler - 市价单处理器
│   ├── StopOrderHandler - 止损单处理器
│   ├── StopLimitOrderHandler - 止损限价单处理器
│   ├── ImmediateOrderHandler - IOC/FOK订单处理器
│   ├── IcebergOrderHandler - 冰山订单处理器
│   ├── DayOrderHandler - 当日有效订单处理器
│   └── GTCOrderHandler - 取消前有效订单处理器
├── order_processor_threads - 订单处理线程池
└── matching_threads - 撮合执行线程
```

## 快速开始

### 创建订单簿

```rust
use orderbook::OrderBook;
use std::sync::Arc;

fn main() {
// 创建新的订单簿
let orderbook = Arc::new(OrderBook::new("BTC/USDT"));

    // 设置撮合回调
    orderbook.set_match_callback(|match_result| {
        println!("撮合成交: {:?}", match_result);
    });

}
```

### 添加订单

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

// 添加单个订单
let order = create_order();
orderbook.add_order_async(order.clone())?;

// 批量添加订单
let orders = vec![order1, order2, order3];
orderbook.add_orders_batch(orders)?;
```

### 查询订单簿状态

```rust
// 获取市场深度
let depth = orderbook.get_market_depth(10);
println!("买盘: {:?}", depth.bids);
println!("卖盘: {:?}", depth.asks);

// 获取统计数据
let stats = orderbook.get_stats();
println!("总订单数: {}, 活跃订单: {}", stats.0, stats.1);

// 获取队列状态
let queue_status = orderbook.get_queue_status();
println!("通道大小: {}, 处理器数: {}, 撮合器数: {}",
queue_status.0, queue_status.1, queue_status.2);

// 查找订单
if let Some(order) = orderbook.find_order("order_001") {
println!("找到订单: {:?}", order);
}
```

### 不同订单类型示例

```rust
// 市价单
let market_order = Order {
order_type: OrderType::Market,
// ... 其他字段
};

// 冰山订单
let iceberg_order = Order {
order_type: OrderType::Iceberg,
quantity: AtomicF64::new(1000.0), // 总量
// 引擎会自动拆分为小单
};

// 止损单
let stop_order = Order {
order_type: OrderType::Stop,
price: AtomicF64::new(45000.0), // 触发价格
direction: OrderDirection::Sell,
// 当价格 <= 45000 时触发
};

// IOC 订单（立即或取消）
let ioc_order = Order {
order_type: OrderType::IOC,
// 如果不能立即成交则取消
};
```

### 关闭订单簿

```rust
// 手动关闭
orderbook.shutdown();

// 或依赖 Drop 自动关闭
drop(orderbook);
```

## 性能特性

- **无锁设计**：消除锁竞争，提高多核 CPU 利用率
- **内存安全**：基于 Rust 的所有权系统，避免数据竞争
- **实时性**：微秒级撮合延迟
- **可扩展**：自动根据 CPU 核心数调整线程数量

## 核心 API

- `OrderBook::new(symbol: &str)` - 创建订单簿
- `add_order_async(order: Arc<Order>)` - 异步添加订单
- `add_orders_batch(orders: Vec<Arc<Order>>)` - 批量添加订单
- `get_market_depth(levels: usize)` - 获取市场深度
- `find_order(order_id: &str)` - 查找订单
- `get_stats()` - 获取统计信息
- `get_queue_status()` - 获取队列状态
- `shutdown()` - 关闭订单簿
