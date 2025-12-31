// File: basic_test_1.rs
// Basic tests for the lock-free order book.

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

    // Helper function to create test orders
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

    // Function to clear the screen
    fn clear_screen() {
        print!("{esc}c", esc = 27 as char);
    }

    #[test]
    fn test_orderbook_basic_operations() {
        println!("🔧 Testing order book basic operations...");
        let orderbook = OrderBook::new("BTC/USDT");
        println!("📝 Test 1: Adding buy order");
        let buy_order = create_test_order(1, OrderDirection::Buy, 45000.0, 1.5);
        let result = orderbook.add_order(buy_order.clone());
        assert!(
            result.is_ok(),
            "Failed to add buy order: {:?}",
            result.err()
        );
        println!("📝 Test 2: Adding sell order");
        let sell_order = create_test_order(2, OrderDirection::Sell, 46000.0, 2.0);
        let result = orderbook.add_order(sell_order.clone());
        assert!(
            result.is_ok(),
            "Failed to add sell order: {:?}",
            result.err()
        );
        println!("📝 Test 3: Finding order");
        let found_order = orderbook.find_order("ORDER-1");
        assert!(found_order.is_some(), "Failed to find order");
        assert_eq!(
            found_order.unwrap().id,
            "ORDER-1",
            "Found order ID does not match"
        );
        println!("📝 Test 4: Getting statistics");
        let stats = orderbook.get_stats();
        println!(
            "Statistics: Total orders={}, Active orders={}, Current orders={}",
            stats.0, stats.1, stats.2
        );
        println!("📝 Test 5: Getting market depth");
        let depth = orderbook.get_market_depth(5);
        println!("Bid depth: {} levels", depth.bids.len());
        println!("Ask depth: {} levels", depth.asks.len());
        println!("✅ Basic operations test passed!");
    }

    #[test]
    fn test_orderbook_simplest() {
        println!("🧪 Testing order book simplest logic...");
        // 1. Create order book
        let orderbook = OrderBook::new("TEST/USD");
        println!("✅ Order book created successfully");
        // 2. Create a buy order
        let buy_order = create_test_order(1, OrderDirection::Buy, 100.0, 1.0);
        let result = orderbook.add_order(buy_order.clone());
        assert!(
            result.is_ok(),
            "Failed to add buy order: {:?}",
            result.err()
        );
        println!("✅ Buy order added successfully: ID={}", buy_order.id);
        // 3. Create a sell order
        let sell_order = create_test_order(2, OrderDirection::Sell, 101.0, 1.5);
        let result = orderbook.add_order(sell_order.clone());
        assert!(
            result.is_ok(),
            "Failed to add sell order: {:?}",
            result.err()
        );
        println!("✅ Sell order added successfully: ID={}", sell_order.id);
        // 4. Find orders
        let found_buy = orderbook.find_order("ORDER-1");
        assert!(found_buy.is_some(), "Failed to find buy order");
        assert_eq!(found_buy.unwrap().id, "ORDER-1");
        println!("✅ Buy order found successfully");
        let found_sell = orderbook.find_order("ORDER-2");
        assert!(found_sell.is_some(), "Failed to find sell order");
        assert_eq!(found_sell.unwrap().id, "ORDER-2");
        println!("✅ Sell order found successfully");
        // 5. Get statistics
        let stats = orderbook.get_stats();
        println!(
            "📊 Statistics: Total orders={}, Active orders={}, Current orders={}",
            stats.0, stats.1, stats.2
        );
        assert!(stats.0 >= 2, "Incorrect total order count");
        assert!(stats.1 >= 2, "Incorrect active order count");
        // 6. Get market depth
        let depth = orderbook.get_market_depth(3);
        println!(
            "📈 Bid depth: {} levels, Ask depth: {} levels",
            depth.bids.len(),
            depth.asks.len()
        );
        println!("🎉 All simplest tests passed!");
    }

    #[test]
    fn test_orderbook_realtime_display() {
        println!("📊 Testing order book real-time display...");

        // Create order book
        let orderbook = Arc::new(OrderBook::new("BTC/USDT"));
        let orderbook_clone = orderbook.clone();

        // Order counter
        let order_counter = Arc::new(AtomicUsize::new(1));

        // Start order generation thread
        let producer_thread = {
            let orderbook = orderbook.clone();
            let order_counter = order_counter.clone();

            thread::spawn(move || {
                let mut rng = thread_rng();
                let mut order_id = 1;

                println!("🚀 Starting order generation...");

                for _ in 0..20 {
                    // Generate 20 orders
                    thread::sleep(Duration::from_millis(500)); // Generate one every 0.5 seconds

                    let direction = if rng.gen_bool(0.5) {
                        OrderDirection::Buy
                    } else {
                        OrderDirection::Sell
                    };

                    // Generate random price and quantity
                    let price = if direction == OrderDirection::Buy {
                        rng.gen_range(45000.0..45500.0)
                    } else {
                        rng.gen_range(45600.0..46000.0)
                    };

                    let quantity = rng.gen_range(0.1..5.0);

                    // Create and add order
                    let order = create_test_order(order_id as u64, direction, price, quantity);
                    let result = orderbook.add_order(order.clone());

                    if result.is_ok() {
                        println!(
                            "📨 Added order: ID={}, Direction={:?}, Price={:.2}, Quantity={:.4}",
                            order.id, direction, price, quantity
                        );
                        order_counter.fetch_add(1, Ordering::Relaxed);
                        order_id += 1;
                    } else {
                        println!("❌ Failed to add order: {:?}", result.err());
                    }
                }

                println!("🛑 Order generation completed");
            })
        };

        // Function to display order book status (with in-line updates)
        fn display_orderbook_status(orderbook: &OrderBook, counter: &AtomicUsize) {
            let stats = orderbook.get_stats();
            let total_orders = stats.0;
            let active_orders = stats.1;
            let current_orders = stats.2;
            // Get market depth
            let depth = orderbook.get_market_depth(5);
            // Build display table (overwrites previous output)
            print!("\x1B[2J\x1B[1;1H"); // Clear screen and move cursor to top-left
            println!("┌─────────────────────────────────────────────────────┐");
            println!("│               📊 Order Book Real-Time Monitor       │");
            println!("├─────────────────────────────────────────────────────┤");
            println!("│ Symbol: BTC/USDT                                    │");
            println!(
                "│ Time: {:?}                                    │",
                chrono::Local::now().format("%H:%M:%S")
            );
            println!("├─────────────────────────────────────────────────────┤");
            println!("│                  📈 Order Statistics                │");
            println!("├─────────────┬─────────────┬─────────────┬───────────┤");
            println!("│ Total Orders│ Active Orders│Current Orders│ Sequence │");
            println!("├─────────────┼─────────────┼─────────────┼───────────┤");
            println!(
                "│   {:>6}    │   {:>6}    │   {:>6}    │   {:>6}  │",
                total_orders,
                active_orders,
                current_orders,
                counter.load(Ordering::Relaxed)
            );
            println!("├─────────────┴─────────────┴─────────────┴───────────┤");
            // Display bid depth
            println!("│                  🟢 Bid Depth (5 levels)            │");
            println!("├─────────────┬─────────────┬─────────────┬───────────┤");
            println!("│    Price    │   Quantity  │ Order Count │   Level   │");
            println!("├─────────────┼─────────────┼─────────────┼───────────┤");
            if depth.bids.is_empty() {
                println!("│          No bids available                       │");
            } else {
                for (i, level) in depth.bids.iter().enumerate() {
                    println!(
                        "│ {:>11.2} │ {:>11.4} │ {:>11} │ {:>9} │",
                        level.price as f64 / 100.0, // Assuming price stored as integer
                        level.quantity as f64 / 10000.0, // Assuming quantity stored as integer
                        level.order_count,
                        i + 1
                    );
                }
            }
            // Display ask depth
            println!("├─────────────┴─────────────┴─────────────┴───────────┤");
            println!("│                  🔴 Ask Depth (5 levels)            │");
            println!("├─────────────┬─────────────┬─────────────┬───────────┤");
            println!("│    Price    │   Quantity  │ Order Count │   Level   │");
            println!("├─────────────┼─────────────┼─────────────┼───────────┤");
            if depth.asks.is_empty() {
                println!("│          No asks available                       │");
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
            // Display separator
            println!("════════════════════════════════════════════════════════");
        }
        // Main display loop
        println!("🖥️  Starting real-time order book display...");
        let start_time = Instant::now();
        while start_time.elapsed() < Duration::from_secs(12) {
            display_orderbook_status(&orderbook_clone, &order_counter);
            thread::sleep(Duration::from_millis(1000)); // Refresh every second
        }
        // Wait for producer thread to finish
        let _ = producer_thread.join();
        println!("✅ Real-time display test completed!");
        // Final statistics
        let final_stats = orderbook_clone.get_stats();
        println!("📊 Final statistics:");
        println!("   Total orders: {}", final_stats.0);
        println!("   Active orders: {}", final_stats.1);
        println!("   Current orders: {}", final_stats.2);
    }

    #[test]
    fn test_orderbook_concurrent_access() {
        println!("⚡ Testing order book concurrent access...");
        let orderbook = Arc::new(OrderBook::new("ETH/USDT"));
        let mut handles = vec![];
        // Create multiple threads to concurrently add orders
        for thread_id in 0..5 {
            let orderbook_clone = orderbook.clone();
            let handle = thread::spawn(move || {
                for i in 0..10 {
                    // Each thread adds 10 orders
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
                        println!("Thread{}: Successfully added order {}", thread_id, order.id);
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
        // Verify final state
        let stats = orderbook.get_stats();
        println!("Concurrent test results:");
        println!("  Total orders: {}", stats.0);
        println!("  Active orders: {}", stats.1);
        println!("  Current orders: {}", stats.2);
        assert_eq!(stats.0, 50, "Incorrect total order count");
        assert_eq!(stats.1, 50, "Incorrect active order count");
        println!("✅ Concurrent access test passed!");
    }

    #[test]
    fn test_orderbook_realtime_async_display() {
        println!("🔄 Testing order book asynchronous real-time updates...");
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
                println!("📺 Display thread started (Press Ctrl+C to stop)...");
                let mut last_display = String::new();
                while !stop_flag.load(Ordering::Relaxed) {
                    let stats = orderbook.get_stats();
                    let depth = orderbook.get_market_depth(3);
                    // Build display content
                    let display = format!(
                        "\r📊 Order Book Real-Time Status | Total Orders: {} | Active Orders: {} | Buy Orders: {} | Sell Orders: {} | Bids: {} levels | Asks: {} levels | Time: {}",
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
                    thread::sleep(Duration::from_millis(100)); // Update every 100ms
                }
                println!("\r✅ Display thread stopped");
            })
        };
        let buy_thread = {
            let orderbook = orderbook.clone();
            let stop_flag = stop_flag.clone();
            let buy_counter = buy_counter.clone();
            thread::spawn(move || {
                println!("🛒 Buy order thread started");
                let mut rng = thread_rng();
                let mut buy_id = 1000; // Buy order IDs start from 1000
                while !stop_flag.load(Ordering::Relaxed) {
                    let price = rng.gen_range(45000.0..45200.0);
                    let quantity = rng.gen_range(0.1..5.0);
                    let order = create_test_order(buy_id, OrderDirection::Buy, price, quantity);
                    if let Err(e) = orderbook.add_order(order.clone()) {
                        eprintln!("\r❌ Failed to add buy order: {}", e);
                    } else {
                        let count = buy_counter.fetch_add(1, Ordering::Relaxed);
                        if count % 10 == 0 {
                            print!("\r🛒 Added {} buy orders", count);
                            std::io::Write::flush(&mut std::io::stdout()).unwrap();
                        }
                    }
                    buy_id += 1;
                    thread::sleep(Duration::from_millis(200)); // Add a buy order every 200ms
                }
                println!("\r✅ Buy order thread stopped");
            })
        };
        let sell_thread = {
            let orderbook = orderbook.clone();
            let stop_flag = stop_flag.clone();
            let sell_counter = sell_counter.clone();
            thread::spawn(move || {
                println!("🏷️ Sell order thread started");
                let mut rng = thread_rng();
                let mut sell_id = 2000; // Sell order IDs start from 2000
                while !stop_flag.load(Ordering::Relaxed) {
                    let price = rng.gen_range(45250.0..45500.0);
                    let quantity = rng.gen_range(0.1..3.0);
                    let order = create_test_order(sell_id, OrderDirection::Sell, price, quantity);
                    if let Err(e) = orderbook.add_order(order.clone()) {
                        eprintln!("\r❌ Failed to add sell order: {}", e);
                    } else {
                        let count = sell_counter.fetch_add(1, Ordering::Relaxed);
                        print!("\r🏷️ Added {} sell orders", count);
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                    sell_id += 1;
                    thread::sleep(Duration::from_secs(1)); // Add a sell order every second
                }
                println!("\r✅ Sell order thread stopped");
            })
        };
        println!("\n⏱️  Test running for 10 seconds...");
        thread::sleep(Duration::from_secs(10));
        stop_flag.store(true, Ordering::Relaxed);
        println!("\n🛑 Stopping all threads...");
        let _ = display_thread.join();
        let _ = buy_thread.join();
        let _ = sell_thread.join();
        println!("\n📊 Final statistics:");
        let stats = orderbook.get_stats();
        let depth = orderbook.get_market_depth(5);
        println!("   Total orders: {}", stats.0);
        println!("   Active orders: {}", stats.1);
        println!(
            "   Buy order count: {}",
            buy_counter.load(Ordering::Relaxed)
        );
        println!(
            "   Sell order count: {}",
            sell_counter.load(Ordering::Relaxed)
        );
        println!("   Bid depth: {} levels", depth.bids.len());
        println!("   Ask depth: {} levels", depth.asks.len());
        if !depth.bids.is_empty() {
            println!("\n🟢 Top 3 bid levels:");
            for (i, level) in depth.bids.iter().take(3).enumerate() {
                println!(
                    "   Level {}: Price={:.2}, Quantity={:.4}, Order Count={}",
                    i + 1,
                    level.price as f64 / 100.0,
                    level.quantity as f64 / 10000.0,
                    level.order_count
                );
            }
        }
        if !depth.asks.is_empty() {
            println!("\n🔴 Top 3 ask levels:");
            for (i, level) in depth.asks.iter().take(3).enumerate() {
                println!(
                    "   Level {}: Price={:.2}, Quantity={:.4}, Order Count={}",
                    i + 1,
                    level.price as f64 / 100.0,
                    level.quantity as f64 / 10000.0,
                    level.order_count
                );
            }
        }
        println!("\n🎉 Asynchronous real-time test completed!");
    }
}
