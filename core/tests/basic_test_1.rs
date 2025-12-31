#[cfg(test)]
mod basic_test_1 {
    use atomic_plus::AtomicF64;
    use luminengine::orderbook::OrderBook;
    use luminengine::orderbook::order::{
        AtomicOrderStatus, Order, OrderDirection, OrderStatus, OrderType,
    };

    use super::*;
    use rand::{Rng, thread_rng};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    // 创建测试订单的辅助函数
    fn create_test_order(
        id: u64,
        direction: OrderDirection,
        price: f64,
        quantity: f64,
    ) -> Arc<Order> {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        Arc::new(Order {
            id: format!("ORDER-{}", id),
            symbol: "BTC/USDT".to_string(),
            price: AtomicF64::new(price),
            direction,
            quantity: AtomicF64::new(quantity),
            remaining: AtomicF64::new(quantity),
            filled: AtomicF64::new(0.0),
            crt_time: chrono::Utc::now().to_rfc3339(),
            status: AtomicOrderStatus::new(OrderStatus::Pending),
            expiry: None,
            order_type: OrderType::Limit,
            ex: Some("test".to_string()),
            version: AtomicU64::new(1),
            timestamp_ns,
            parent_order_id: None,
            priority: 1,
        })
    }

    // 清除控制台显示的函数
    fn clear_screen() {
        print!("{esc}c", esc = 27 as char);
    }

    #[test]
    fn test_orderbook_basic_operations() {
        println!("🔧 测试订单簿基本操作...");
        let orderbook = OrderBook::new("BTC/USDT");
        println!("📝 测试1: 添加买单");
        let buy_order = create_test_order(1, OrderDirection::Buy, 45000.0, 1.5);
        let result = orderbook.add_order(buy_order.clone());
        assert!(result.is_ok(), "添加买单失败: {:?}", result.err());
        println!("📝 测试2: 添加卖单");
        let sell_order = create_test_order(2, OrderDirection::Sell, 46000.0, 2.0);
        let result = orderbook.add_order(sell_order.clone());
        assert!(result.is_ok(), "添加卖单失败: {:?}", result.err());
        println!("📝 测试3: 查找订单");
        let found_order = orderbook.find_order("ORDER-1");
        assert!(found_order.is_some(), "查找订单失败");
        assert_eq!(found_order.unwrap().id, "ORDER-1", "找到的订单ID不匹配");
        println!("📝 测试4: 获取统计信息");
        let stats = orderbook.get_stats();
        println!(
            "统计信息: 总订单={}, 活跃订单={}, 当前订单数={}",
            stats.0, stats.1, stats.2
        );
        println!("📝 测试5: 获取市场深度");
        let depth = orderbook.get_market_depth(5);
        println!("买单深度: {} 层", depth.bids.len());
        println!("卖单深度: {} 层", depth.asks.len());
        println!("✅ 基本操作测试通过！");
    }

    #[test]
    fn test_orderbook_simplest() {
        println!("🧪 测试订单簿最简逻辑...");
        // 1. 创建订单簿
        let orderbook = OrderBook::new("TEST/USD");
        println!("✅ 订单簿创建成功");
        // 2. 创建一个买单
        let buy_order = create_test_order(1, OrderDirection::Buy, 100.0, 1.0);
        let result = orderbook.add_order(buy_order.clone());
        assert!(result.is_ok(), "添加买单失败: {:?}", result.err());
        println!("✅ 买单添加成功: ID={}", buy_order.id);
        // 3. 创建一个卖单
        let sell_order = create_test_order(2, OrderDirection::Sell, 101.0, 1.5);
        let result = orderbook.add_order(sell_order.clone());
        assert!(result.is_ok(), "添加卖单失败: {:?}", result.err());
        println!("✅ 卖单添加成功: ID={}", sell_order.id);
        // 4. 查找订单
        let found_buy = orderbook.find_order("ORDER-1");
        assert!(found_buy.is_some(), "查找买单失败");
        assert_eq!(found_buy.unwrap().id, "ORDER-1");
        println!("✅ 买单查找成功");
        let found_sell = orderbook.find_order("ORDER-2");
        assert!(found_sell.is_some(), "查找卖单失败");
        assert_eq!(found_sell.unwrap().id, "ORDER-2");
        println!("✅ 卖单查找成功");
        // 5. 获取统计
        let stats = orderbook.get_stats();
        println!(
            "📊 统计: 总订单={}, 活跃订单={}, 当前订单={}",
            stats.0, stats.1, stats.2
        );
        assert!(stats.0 >= 2, "总订单数不正确");
        assert!(stats.1 >= 2, "活跃订单数不正确");
        // 6. 获取市场深度
        let depth = orderbook.get_market_depth(3);
        println!(
            "📈 买单深度: {}层, 卖单深度: {}层",
            depth.bids.len(),
            depth.asks.len()
        );
        println!("🎉 最简测试全部通过！");
    }

    #[test]
    fn test_orderbook_realtime_display() {
        println!("📊 测试订单簿实时显示...");

        // 创建订单簿
        let orderbook = Arc::new(OrderBook::new("BTC/USDT"));
        let orderbook_clone = orderbook.clone();

        // 订单计数器
        let order_counter = Arc::new(AtomicUsize::new(1));

        // 启动订单生成线程
        let producer_thread = {
            let orderbook = orderbook.clone();
            let order_counter = order_counter.clone();

            thread::spawn(move || {
                let mut rng = thread_rng();
                let mut order_id = 1;

                println!("🚀 开始生成订单...");

                for _ in 0..20 {
                    // 生成20个订单
                    thread::sleep(Duration::from_millis(500)); // 每0.5秒生成一个

                    let direction = if rng.gen_bool(0.5) {
                        OrderDirection::Buy
                    } else {
                        OrderDirection::Sell
                    };

                    // 生成随机价格和数量
                    let price = if direction == OrderDirection::Buy {
                        rng.gen_range(45000.0..45500.0)
                    } else {
                        rng.gen_range(45600.0..46000.0)
                    };

                    let quantity = rng.gen_range(0.1..5.0);

                    // 创建并添加订单
                    let order = create_test_order(order_id as u64, direction, price, quantity);
                    let result = orderbook.add_order(order.clone());

                    if result.is_ok() {
                        println!(
                            "📨 已添加订单: ID={}, 方向={:?}, 价格={:.2}, 数量={:.4}",
                            order.id, direction, price, quantity
                        );
                        order_counter.fetch_add(1, Ordering::Relaxed);
                        order_id += 1;
                    } else {
                        println!("❌ 添加订单失败: {:?}", result.err());
                    }
                }

                println!("🛑 订单生成完成");
            })
        };

        // 显示订单簿状态的函数
        fn display_orderbook_status(orderbook: &OrderBook, counter: &AtomicUsize) {
            let stats = orderbook.get_stats();
            let total_orders = stats.0;
            let active_orders = stats.1;
            let current_orders = stats.2;

            // 获取市场深度
            let depth = orderbook.get_market_depth(5);

            // 构建显示表格
            println!("\n\n");
            println!("┌─────────────────────────────────────────────────────┐");
            println!("│               📊 订单簿实时监控系统                  │");
            println!("├─────────────────────────────────────────────────────┤");
            println!("│ 交易对: BTC/USDT                                    │");
            println!(
                "│ 时间: {:?}                                    │",
                chrono::Local::now().format("%H:%M:%S")
            );
            println!("├─────────────────────────────────────────────────────┤");
            println!("│                  📈 订单统计                          │");
            println!("├─────────────┬─────────────┬─────────────┬───────────┤");
            println!("│  总订单数   │  活跃订单   │  当前订单   │  序号     │");
            println!("├─────────────┼─────────────┼─────────────┼───────────┤");
            println!(
                "│   {:>6}    │   {:>6}    │   {:>6}    │   {:>6}  │",
                total_orders,
                active_orders,
                current_orders,
                counter.load(Ordering::Relaxed)
            );
            println!("├─────────────┴─────────────┴─────────────┴───────────┤");

            // 显示买单深度
            println!("│                  🟢 买单深度 (5档)                   │");
            println!("├─────────────┬─────────────┬─────────────┬───────────┤");
            println!("│    价格     │    数量     │   订单数     │   级别    │");
            println!("├─────────────┼─────────────┼─────────────┼───────────┤");

            if depth.bids.is_empty() {
                println!("│          暂无买单                             │");
            } else {
                for (i, level) in depth.bids.iter().enumerate() {
                    println!(
                        "│ {:>11.2} │ {:>11.4} │ {:>11} │ {:>9} │",
                        level.price as f64 / 100.0, // 假设价格以整数存储，转换为浮点数
                        level.quantity as f64 / 10000.0, // 假设数量以整数存储
                        level.order_count,
                        i + 1
                    );
                }
            }

            // 显示卖单深度
            println!("├─────────────┴─────────────┴─────────────┴───────────┤");
            println!("│                  🔴 卖单深度 (5档)                   │");
            println!("├─────────────┬─────────────┬─────────────┬───────────┤");
            println!("│    价格     │    数量     │   订单数     │   级别    │");
            println!("├─────────────┼─────────────┼─────────────┼───────────┤");

            if depth.asks.is_empty() {
                println!("│          暂无卖单                             │");
            } else {
                for (i, level) in depth.asks.iter().enumerate() {
                    println!(
                        "│ {:>11.2} │ {:>11.4} │ {:>11} │ {:>9} │",
                        level.price as f64 / 100.0,
                        level.quantity as f64 / 10000.0,
                        level.order_count,
                        i + 1
                    );
                }
            }

            println!("└─────────────────────────────────────────────────────┘");

            // 显示分隔线
            println!("════════════════════════════════════════════════════════");
        }

        // 主显示循环
        println!("🖥️  开始实时显示订单簿状态...");
        let start_time = Instant::now();

        while start_time.elapsed() < Duration::from_secs(12) {
            clear_screen();
            display_orderbook_status(&orderbook_clone, &order_counter);
            thread::sleep(Duration::from_millis(1000)); // 每秒刷新一次
        }

        // 等待生产者线程结束
        let _ = producer_thread.join();

        println!("✅ 实时显示测试完成！");

        // 最终统计
        let final_stats = orderbook_clone.get_stats();
        println!("📊 最终统计:");
        println!("   总订单数: {}", final_stats.0);
        println!("   活跃订单: {}", final_stats.1);
        println!("   当前订单: {}", final_stats.2);
    }

    #[test]
    fn test_orderbook_concurrent_access() {
        println!("⚡ 测试订单簿并发访问...");

        let orderbook = Arc::new(OrderBook::new("ETH/USDT"));
        let mut handles = vec![];

        // 创建多个线程并发添加订单
        for thread_id in 0..5 {
            let orderbook_clone = orderbook.clone();
            let handle = thread::spawn(move || {
                for i in 0..10 {
                    // 每个线程添加10个订单
                    let order_num = thread_id * 10 + i + 1;
                    let direction = if order_num % 2 == 0 {
                        OrderDirection::Buy
                    } else {
                        OrderDirection::Sell
                    };

                    let price = if direction == OrderDirection::Buy {
                        2500.0 + (order_num as f64 * 0.5)
                    } else {
                        2550.0 + (order_num as f64 * 0.5)
                    };

                    let quantity = 1.0 + (order_num as f64 * 0.1);

                    let order = create_test_order(order_num as u64, direction, price, quantity);
                    let result = orderbook_clone.add_order(order.clone());

                    if result.is_ok() {
                        println!("线程{}: 成功添加订单 {}", thread_id, order.id);
                    }

                    thread::sleep(Duration::from_millis(10));
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证最终状态
        let stats = orderbook.get_stats();
        println!("并发测试结果:");
        println!("  总订单数: {}", stats.0);
        println!("  活跃订单: {}", stats.1);
        println!("  当前订单: {}", stats.2);

        assert_eq!(stats.0, 50, "总订单数不正确");
        assert_eq!(stats.1, 50, "活跃订单数不正确");

        println!("✅ 并发访问测试通过！");
    }

    #[test]
    fn test_orderbook_realtime_async_display() {
        println!("🔄 测试订单簿异步实时更新...");
        let orderbook = Arc::new(OrderBook::new("BTC/USDT"));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let buy_counter = Arc::new(AtomicU64::new(0));
        let sell_counter = Arc::new(AtomicU64::new(0));
        let display_thread = {
            let orderbook = orderbook.clone();
            let stop_flag = stop_flag.clone();
            let buy_counter = buy_counter.clone();
            let sell_counter = sell_counter.clone();
            thread::spawn(move || {
                println!("📺 显示线程启动 (按 Ctrl+C 停止)...");
                let mut last_display = String::new();
                while !stop_flag.load(Ordering::Relaxed) {
                    let stats = orderbook.get_stats();
                    let depth = orderbook.get_market_depth(3);
                    // 构建显示内容
                    let display = format!(
                        "\r📊 订单簿实时状态 | 总订单: {} | 活跃订单: {} | 买单: {} | 卖单: {} | 买盘: {}层 | 卖盘: {}层 | 时间: {}",
                        stats.0,
                        stats.1,
                        buy_counter.load(Ordering::Relaxed),
                        sell_counter.load(Ordering::Relaxed),
                        depth.bids.len(),
                        depth.asks.len(),
                        chrono::Local::now().format("%H:%M:%S.%3f")
                    );
                    if display != last_display {
                        print!("{}", display);
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                        last_display = display;
                    }
                    thread::sleep(Duration::from_millis(100)); // 每100ms更新一次
                }
                println!("\r✅ 显示线程停止");
            })
        };
        let buy_thread = {
            let orderbook = orderbook.clone();
            let stop_flag = stop_flag.clone();
            let buy_counter = buy_counter.clone();
            thread::spawn(move || {
                println!("🛒 买单线程启动");
                let mut rng = thread_rng();
                let mut buy_id = 1000; // 买单ID从1000开始
                while !stop_flag.load(Ordering::Relaxed) {
                    let price = rng.gen_range(45000.0..45200.0);
                    let quantity = rng.gen_range(0.1..5.0);
                    let order = create_test_order(buy_id, OrderDirection::Buy, price, quantity);
                    if let Err(e) = orderbook.add_order(order.clone()) {
                        eprintln!("\r❌ 添加买单失败: {}", e);
                    } else {
                        let count = buy_counter.fetch_add(1, Ordering::Relaxed);
                        if count % 10 == 0 {
                            print!("\r🛒 已添加 {} 个买单", count);
                            std::io::Write::flush(&mut std::io::stdout()).unwrap();
                        }
                    }
                    buy_id += 1;
                    thread::sleep(Duration::from_millis(200)); // 每200ms添加一个买单
                }
                println!("\r✅ 买单线程停止");
            })
        };
        let sell_thread = {
            let orderbook = orderbook.clone();
            let stop_flag = stop_flag.clone();
            let sell_counter = sell_counter.clone();
            thread::spawn(move || {
                println!("🏷️ 卖单线程启动");
                let mut rng = thread_rng();
                let mut sell_id = 2000; // 卖单ID从2000开始
                while !stop_flag.load(Ordering::Relaxed) {
                    let price = rng.gen_range(45250.0..45500.0);
                    let quantity = rng.gen_range(0.1..3.0);
                    let order = create_test_order(sell_id, OrderDirection::Sell, price, quantity);
                    if let Err(e) = orderbook.add_order(order.clone()) {
                        eprintln!("\r❌ 添加卖单失败: {}", e);
                    } else {
                        let count = sell_counter.fetch_add(1, Ordering::Relaxed);
                        print!("\r🏷️ 已添加 {} 个卖单", count);
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                    sell_id += 1;
                    thread::sleep(Duration::from_secs(1)); // 每秒添加一个卖单
                }
                println!("\r✅ 卖单线程停止");
            })
        };
        println!("\n⏱️  测试运行10秒...");
        thread::sleep(Duration::from_secs(10));
        stop_flag.store(true, Ordering::Relaxed);
        println!("\n🛑 停止所有线程...");
        let _ = display_thread.join();
        let _ = buy_thread.join();
        let _ = sell_thread.join();
        println!("\n📊 最终统计:");
        let stats = orderbook.get_stats();
        let depth = orderbook.get_market_depth(5);
        println!("   总订单数: {}", stats.0);
        println!("   活跃订单: {}", stats.1);
        println!("   买单数量: {}", buy_counter.load(Ordering::Relaxed));
        println!("   卖单数量: {}", sell_counter.load(Ordering::Relaxed));
        println!("   买盘深度: {} 层", depth.bids.len());
        println!("   卖盘深度: {} 层", depth.asks.len());
        if !depth.bids.is_empty() {
            println!("\n🟢 买盘前3档:");
            for (i, level) in depth.bids.iter().take(3).enumerate() {
                println!(
                    "   {}档: 价格={:.2}, 数量={:.4}, 订单数={}",
                    i + 1,
                    level.price as f64 / 100.0,
                    level.quantity as f64 / 10000.0,
                    level.order_count
                );
            }
        }
        if !depth.asks.is_empty() {
            println!("\n🔴 卖盘前3档:");
            for (i, level) in depth.asks.iter().take(3).enumerate() {
                println!(
                    "   {}档: 价格={:.2}, 数量={:.4}, 订单数={}",
                    i + 1,
                    level.price as f64 / 100.0,
                    level.quantity as f64 / 10000.0,
                    level.order_count
                );
            }
        }
        println!("\n🎉 异步实时测试完成！");
    }
}
