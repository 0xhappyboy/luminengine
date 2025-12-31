#[cfg(test)]
mod tests {
    use super::*;
    use atomic_plus::AtomicF64;
    use luminengine::orderbook::order::{Order, OrderDirection, OrderStatus, OrderType};
    use luminengine::orderbook::{OrderBook, order::AtomicOrderStatus};
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, atomic::AtomicU64};
    use std::time::{Duration, Instant};

    /// Performance test: Add 2 million orders of any type to the order book
    #[test]
    fn test_performance_2m_orders() {
        println!("🚀 === Starting performance test: Adding 2 million orders of any type ===");

        // 1. Create order book
        let start_create = Instant::now();
        let orderbook = OrderBook::new("PERF_TEST");
        let creation_time = start_create.elapsed();
        println!("✓ Order book created, time taken: {:?}", creation_time);

        // 2. Set up simplified match callback
        let total_matches = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let match_count = total_matches.clone();

        // Fix: Clone before creating closure
        let match_count_for_callback = match_count.clone();
        orderbook.set_match_callback(move |result| {
            match_count_for_callback.fetch_add(1, Ordering::Relaxed);
        });

        // 3. Prepare to add 2 million orders
        const TOTAL_ORDERS: usize = 2_000_000;
        println!("📊 Preparing to add {} orders of any type...", TOTAL_ORDERS);

        let mut rng = StdRng::seed_from_u64(42);

        // Performance statistics
        let orders_added = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let errors = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let batch_size = 10000;

        // 4. Start real-time display thread
        let display_orders_added = orders_added.clone();
        let display_errors = errors.clone();
        let display_matches = match_count.clone(); // Use original match_count
        let start_time = Instant::now();
        let total_orders = TOTAL_ORDERS; // Copy to closure

        // Create stop flag
        let display_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let display_stop_clone = display_stop.clone();

        let display_handle = std::thread::spawn(move || {
            // Clear screen and show initial table
            print!("\x1B[2J\x1B[1;1H");

            while !display_stop_clone.load(Ordering::Relaxed) {
                let current_time = Instant::now();
                let elapsed = current_time.duration_since(start_time);
                let current_orders = display_orders_added.load(Ordering::Relaxed);
                let current_errors = display_errors.load(Ordering::Relaxed);
                let current_matches = display_matches.load(Ordering::Relaxed);

                // Calculate rates
                let orders_per_sec = if elapsed.as_secs_f64() > 0.0 {
                    current_orders as f64 / elapsed.as_secs_f64()
                } else {
                    0.0
                };

                let matches_per_sec = if elapsed.as_secs_f64() > 0.0 {
                    current_matches as f64 / elapsed.as_secs_f64()
                } else {
                    0.0
                };

                let error_rate = if current_orders > 0 {
                    (current_errors as f64 / current_orders as f64) * 100.0
                } else {
                    0.0
                };

                let progress = if total_orders > 0 {
                    (current_orders as f64 / total_orders as f64) * 100.0
                } else {
                    0.0
                };

                // Clear screen and update table
                print!("\x1B[2J\x1B[1;1H");

                println!("🚀 === Real-time Performance Monitoring (updated every 1 second) ===");
                println!("┌──────────────────────────────────────────────────────────────┐");
                println!("│ Metric                   │ Current Value      │ Total Value     │");
                println!("├──────────────────────────┼────────────────────┼─────────────────┤");
                println!(
                    "│ Run Time                 │ {:>15?} │                 │",
                    elapsed
                );
                println!("├──────────────────────────┼────────────────────┼─────────────────┤");
                println!(
                    "│ Order Add Rate           │ {:>10.0} orders/sec │                 │",
                    orders_per_sec
                );
                println!(
                    "│ Match Rate               │ {:>10.0} matches/sec │                 │",
                    matches_per_sec
                );
                println!("├──────────────────────────┼────────────────────┼─────────────────┤");
                println!(
                    "│ Target Orders            │                   │ {:>10} / {} │",
                    current_orders, total_orders
                );
                println!(
                    "│ Successfully Added       │                   │ {:>16} │",
                    current_orders
                );
                println!(
                    "│ Failed Orders            │                   │ {:>16} │",
                    current_errors
                );
                println!(
                    "│ Total Matches            │                   │ {:>16} │",
                    current_matches
                );
                println!("├──────────────────────────┼────────────────────┼─────────────────┤");
                println!(
                    "│ Progress                 │ {:>15.1}% │                 │",
                    progress
                );
                println!(
                    "│ Error Rate               │ {:>15.4}% │                 │",
                    error_rate
                );
                println!("└──────────────────────────────────────────────────────────────┘");

                // Progress bar
                println!("\n📊 Processing Progress:");
                print!("[");
                let bar_width = 50;
                let filled_width = (progress as usize * bar_width / 100).min(bar_width);
                for i in 0..bar_width {
                    if i < filled_width {
                        print!("█");
                    } else if i == filled_width && progress < 100.0 {
                        print!("▶");
                    } else {
                        print!(" ");
                    }
                }
                println!("] {:.1}%", progress);

                // Memory usage estimate
                let estimated_memory_mb = (current_orders * 256) as f64 / 1024.0 / 1024.0;
                println!("💾 Memory Estimate: {:.2} MB", estimated_memory_mb);
                println!("📅 Start Time: {:?}", start_time);
                println!("⏰ Last Update: {:?}", current_time);

                // Show current activity
                if current_matches > 0 {
                    println!("⚡ Activity: Matching engine processing orders...");
                } else if current_orders < total_orders {
                    println!("⚡ Activity: Adding orders...");
                } else {
                    println!("⚡ Activity: Waiting for matching to complete...");
                }

                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            }

            // Clear screen when thread ends
            print!("\x1B[2J\x1B[1;1H");
        });

        let start_total = Instant::now();

        // 5. Add orders in batches
        println!("\n⚡ Starting to add orders (real-time updates above table)...");

        for batch in 0..((TOTAL_ORDERS + batch_size - 1) / batch_size) {
            let batch_start = Instant::now();

            for i in 0..batch_size {
                let global_index = batch * batch_size + i;
                if global_index >= TOTAL_ORDERS {
                    break;
                }

                // Randomly generate order type
                let order_type = match rng.gen_range(0..100) {
                    0..=40 => OrderType::Limit,
                    41..=60 => OrderType::Market,
                    61..=70 => OrderType::Stop,
                    71..=80 => OrderType::StopLimit,
                    81..=85 => OrderType::FOK,
                    86..=90 => OrderType::IOC,
                    91..=95 => OrderType::Iceberg,
                    96..=97 => OrderType::DAY,
                    _ => OrderType::GTC,
                };

                let direction = if rng.gen_bool(0.5) {
                    OrderDirection::Buy
                } else {
                    OrderDirection::Sell
                };

                let price = if order_type == OrderType::Market {
                    0.0
                } else {
                    rng.gen_range(100.0..200.0)
                };

                let quantity = rng.gen_range(1.0..1000.0);

                let order = Arc::new(Order {
                    id: format!("PERF_ORDER_{:09}", global_index),
                    symbol: "PERF_TEST".to_string(),
                    price: AtomicF64::new(price),
                    direction,
                    quantity: AtomicF64::new(quantity),
                    remaining: AtomicF64::new(quantity),
                    filled: AtomicF64::new(0.0),
                    crt_time: chrono::Utc::now().to_rfc3339(),
                    status: AtomicOrderStatus::new(OrderStatus::Pending),
                    expiry: match order_type {
                        OrderType::DAY => Some(Instant::now() + Duration::from_secs(86400)),
                        _ => None,
                    },
                    order_type,
                    ex: None,
                    version: AtomicU64::new(0),
                    timestamp_ns: 0,
                    parent_order_id: None,
                    priority: 0,
                });

                match orderbook.add_order(order) {
                    Ok(_) => {
                        orders_added.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }

            let batch_time = batch_start.elapsed();
            if batch_time < Duration::from_micros(10) {}
        }

        // 6. Wait for matching engine to process
        println!("\n⏳ Waiting for matching engine to process remaining orders (table continues real-time updates)...");

        let wait_start = Instant::now();
        let mut no_progress_count = 0;
        let mut last_match_count = match_count.load(Ordering::Relaxed);

        for wait_sec in 1..=10 {
            let current_matches = match_count.load(Ordering::Relaxed);
            let new_matches = current_matches - last_match_count;

            if new_matches == 0 {
                no_progress_count += 1;
                if no_progress_count >= 3 {
                    // Stop display thread
                    display_stop.store(true, Ordering::Relaxed);
                    break;
                }
            } else {
                no_progress_count = 0;
            }

            last_match_count = current_matches;
        }

        // Stop display thread
        display_stop.store(true, Ordering::Relaxed);

        // Wait for display thread to end
        let _ = display_handle.join();

        print!("\x1B[2J\x1B[1;1H");
        println!("⚠️  No new matches for 3 consecutive seconds, stopping wait");

        // 7. Collect final statistics
        let total_elapsed = Instant::now().duration_since(start_total);
        let total_matches_count = match_count.load(Ordering::Relaxed);
        let final_orders_added = orders_added.load(Ordering::Relaxed);
        let final_errors = errors.load(Ordering::Relaxed);

        let stats = orderbook.get_stats();
        let match_stats = orderbook.get_match_stats();

        // 8. Print final report
        println!("\n🎉 === Performance test completed! ===");
        println!("📊 Final Statistics Report:");
        println!("┌──────────────────────────────────────────────────────────────┐");
        println!("│ Metric                      │ Value                         │");
        println!("├─────────────────────────────┼───────────────────────────────┤");
        println!("│ Total Run Time              │ {:>31?} │", total_elapsed);
        println!("│ Target Orders               │ {:>31} │", TOTAL_ORDERS);
        println!("│ Successfully Added          │ {:>31} │", final_orders_added);
        println!("│ Failed Orders               │ {:>31} │", final_errors);
        println!(
            "│ Total Matches                │ {:>31} │",
            total_matches_count
        );
        println!("├─────────────────────────────┼───────────────────────────────┤");
        println!(
            "│ Average Order Add Rate       │ {:>28.0} orders/sec │",
            final_orders_added as f64 / total_elapsed.as_secs_f64()
        );
        println!(
            "│ Average Match Rate           │ {:>28.0} matches/sec │",
            total_matches_count as f64 / total_elapsed.as_secs_f64()
        );
        println!(
            "│ Success Rate                 │ {:>31.2}% │",
            (final_orders_added as f64 / TOTAL_ORDERS as f64) * 100.0
        );
        println!("└──────────────────────────────────────────────────────────────┘");

        // Order book statistics
        println!("\n📈 Order Book Statistics:");
        println!("• Total orders: {}", stats.0);
        println!("• Active orders: {}", stats.1);
        println!("• Orders in order book: {}", stats.2);
        println!(
            "• Bid orders: {}",
            orderbook.bids.size.load(Ordering::Relaxed)
        );
        println!(
            "• Ask orders: {}",
            orderbook.asks.size.load(Ordering::Relaxed)
        );

        // Matching engine statistics
        println!("\n⚡ Matching Engine Statistics:");
        println!(
            "• Total volume: {}",
            match_stats.total_volume.load(Ordering::Relaxed)
        );
        println!(
            "• Total notional: {}",
            match_stats.total_notional.load(Ordering::Relaxed)
        );
        println!(
            "• Orders processed: {}",
            match_stats.orders_processed.load(Ordering::Relaxed)
        );
        println!(
            "• Iceberg order slices: {}",
            match_stats.iceberg_slices.load(Ordering::Relaxed)
        );
        println!(
            "• Stop triggers: {}",
            match_stats.stop_triggers.load(Ordering::Relaxed)
        );

        // 9. Verify basic functionality
        println!("\n✅ Functionality Verification:");
        assert!(orderbook.is_running(), "Matching engine should still be running");
        assert!(
            match_stats.orders_processed.load(Ordering::Relaxed) > 0,
            "At least some orders should have been processed"
        );
        println!("  ✓ All basic functionality verification passed");

        // 10. Cleanup
        println!("\n🧹 Cleaning up resources...");
        orderbook.shutdown();
        println!("  ✓ Cleanup completed");

        println!("\n🎉 === Test completed! ===");
    }
}