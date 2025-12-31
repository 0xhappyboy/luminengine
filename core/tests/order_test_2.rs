#[cfg(test)]
mod order_test_2 {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::Instant,
    };

    use atomic_plus::AtomicF64;
    use luminengine::{
        matcher::MatchEngine,
        orderbook::{
            OrderTree,
            order::{AtomicOrderStatus, Order, OrderDirection, OrderStatus, OrderType},
        },
    };

    #[test]
    fn test_random_orders_with_settlement() {
        use rand::Rng;
        use std::collections::HashMap;
        use std::time::{Duration, SystemTime};
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");
        let bids = Arc::new(OrderTree::new(true));
        let asks = Arc::new(OrderTree::new(false));
        let mut rng = rand::thread_rng();
        let mut order_tracker: HashMap<String, Arc<Order>> = HashMap::new();
        let initial_orders = rng.gen_range(5..15);
        println!("Initializing with {} random orders...", initial_orders);
        for i in 0..initial_orders {
            let direction = if i % 2 == 0 {
                OrderDirection::Buy
            } else {
                OrderDirection::Sell
            };
            let order_type = match rng.gen_range(0..6) {
                0 => OrderType::Limit,
                1 => OrderType::Market,
                2 => OrderType::IOC,
                3 => OrderType::FOK,
                4 => OrderType::Stop,
                5 => OrderType::StopLimit,
                _ => OrderType::Limit,
            };
            let price = rng.gen_range(95..=105) as f64;
            let quantity = rng.gen_range(1..=20) as f64;
            let order_id = format!("INIT_{}_{}", direction_str(direction), i);
            let order = Arc::new(Order {
                id: order_id.clone(),
                symbol: "TEST".to_string(),
                price: AtomicF64::new(price),
                direction,
                quantity: AtomicF64::new(quantity),
                remaining: AtomicF64::new(quantity),
                filled: AtomicF64::new(0.0),
                crt_time: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
                    .to_string(),
                status: AtomicOrderStatus::new(OrderStatus::Pending),
                expiry: None,
                order_type,
                ex: None,
                version: AtomicU64::new(1),
                timestamp_ns: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                parent_order_id: None,
                priority: 0,
            });
            let price_u64 = price as u64;
            let is_bid = direction == OrderDirection::Buy;
            if direction == OrderDirection::Buy {
                bids.add_order(order.clone(), price_u64);
            } else {
                asks.add_order(order.clone(), price_u64);
            }
            engine.add_order(order.clone(), price_u64, is_bid).unwrap();
            order_tracker.insert(order_id.clone(), order.clone());
            println!(
                "Added initial order: {} {:?} {:?} @ {} x {}",
                order_id, direction, order_type, price, quantity
            );
        }
        let cycles = rng.gen_range(3..8);
        println!("Starting {} trading cycles...", cycles);
        for cycle in 0..cycles {
            println!("\n=== Cycle {} ===", cycle + 1);
            engine.execute_price_discovery(&bids, &asks, &symbol);
            let new_orders = rng.gen_range(0..4);
            for i in 0..new_orders {
                let direction = if rng.gen_bool(0.5) {
                    OrderDirection::Buy
                } else {
                    OrderDirection::Sell
                };
                let order_type = match rng.gen_range(0..9) {
                    0 => OrderType::Limit,
                    1 => OrderType::Market,
                    2 => OrderType::Stop,
                    3 => OrderType::StopLimit,
                    4 => OrderType::IOC,
                    5 => OrderType::FOK,
                    6 => OrderType::Iceberg,
                    7 => OrderType::DAY,
                    8 => OrderType::GTC,
                    _ => OrderType::Limit,
                };
                let price = if order_type == OrderType::Market {
                    0.0
                } else {
                    rng.gen_range(95..=105) as f64
                };
                let quantity = rng.gen_range(1..=20) as f64;
                let order_id = format!("CYC{}_{}_{}", cycle, direction_str(direction), i);
                let order = Arc::new(Order {
                    id: order_id.clone(),
                    symbol: "TEST".to_string(),
                    price: AtomicF64::new(price),
                    direction,
                    quantity: AtomicF64::new(quantity),
                    remaining: AtomicF64::new(quantity),
                    filled: AtomicF64::new(0.0),
                    crt_time: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                        .to_string(),
                    status: AtomicOrderStatus::new(OrderStatus::Pending),
                    expiry: if order_type == OrderType::DAY {
                        Some(Instant::now() + Duration::from_secs(60))
                    } else {
                        None
                    },
                    order_type,
                    ex: None,
                    version: AtomicU64::new(1),
                    timestamp_ns: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64,
                    parent_order_id: None,
                    priority: rng.gen_range(1..=10),
                });
                let price_u64 = price as u64;
                let is_bid = direction == OrderDirection::Buy;
                if direction == OrderDirection::Buy {
                    bids.add_order(order.clone(), price_u64);
                } else {
                    asks.add_order(order.clone(), price_u64);
                }
                engine.add_order(order.clone(), price_u64, is_bid).unwrap();
                order_tracker.insert(order_id.clone(), order.clone());
                println!(
                    "Added new order: {} {:?} {:?} @ {} x {}",
                    order_id, direction, order_type, price, quantity
                );
            }
            print_current_status(&engine, &order_tracker);
            std::thread::sleep(Duration::from_millis(100));
        }
        println!("\n=== FINAL SETTLEMENT ===");
        print_final_settlement(&engine, &order_tracker);
        let stats = engine.get_stats();
        assert!(
            stats.total_matches.load(Ordering::Relaxed) > 0
                || stats.orders_processed.load(Ordering::Relaxed) > 0
        );
        println!("Test completed successfully!");
    }

    fn direction_str(direction: OrderDirection) -> &'static str {
        match direction {
            OrderDirection::Buy => "BUY",
            OrderDirection::Sell => "SELL",
            OrderDirection::None => "NONE",
        }
    }

    fn print_current_status(engine: &MatchEngine, order_tracker: &HashMap<String, Arc<Order>>) {
        let stats = engine.get_stats();
        let total_orders = order_tracker.len();
        let filled_orders = order_tracker
            .values()
            .filter(|o| o.status.load(Ordering::Relaxed) == OrderStatus::Filled)
            .count();
        let pending_orders = order_tracker
            .values()
            .filter(|o| o.status.load(Ordering::Relaxed) == OrderStatus::Pending)
            .count();
        let cancelled_orders = order_tracker
            .values()
            .filter(|o| o.status.load(Ordering::Relaxed) == OrderStatus::Cancelled)
            .count();
        println!(
            "Orders: Total={}, Filled={}, Pending={}, Cancelled={}",
            total_orders, filled_orders, pending_orders, cancelled_orders
        );
        println!(
            "Matches: {}, Volume: {}, Notional: {}",
            stats.total_matches.load(Ordering::Relaxed),
            stats.total_volume.load(Ordering::Relaxed),
            stats.total_notional.load(Ordering::Relaxed)
        );
    }

    fn print_final_settlement(engine: &MatchEngine, order_tracker: &HashMap<String, Arc<Order>>) {
        let stats = engine.get_stats();

        println!("\n=== MATCHING STATISTICS ===");
        println!(
            "Total Matches: {}",
            stats.total_matches.load(Ordering::Relaxed)
        );
        println!(
            "Total Volume: {}",
            stats.total_volume.load(Ordering::Relaxed)
        );
        println!(
            "Total Notional: {}",
            stats.total_notional.load(Ordering::Relaxed)
        );
        println!(
            "Cross Matches: {}",
            stats.cross_matches.load(Ordering::Relaxed)
        );
        println!(
            "Orders Processed: {}",
            stats.orders_processed.load(Ordering::Relaxed)
        );
        println!(
            "Iceberg Slices: {}",
            stats.iceberg_slices.load(Ordering::Relaxed)
        );
        println!(
            "Stop Triggers: {}",
            stats.stop_triggers.load(Ordering::Relaxed)
        );
        println!(
            "Match Latency (ns): {}",
            stats.match_latency_ns.load(Ordering::Relaxed)
        );

        println!("\n=== ORDER STATUS SUMMARY ===");
        let mut pending_count = 0;
        let mut partial_count = 0;
        let mut filled_count = 0;
        let mut cancelled_count = 0;
        let mut expired_count = 0;

        let mut limit_count = 0;
        let mut market_count = 0;
        let mut stop_count = 0;
        let mut stop_limit_count = 0;
        let mut ioc_count = 0;
        let mut fok_count = 0;
        let mut iceberg_count = 0;
        let mut day_count = 0;
        let mut gtc_count = 0;
        let mut total_filled = 0.0;
        let mut total_remaining = 0.0;
        for (id, order) in order_tracker {
            let status = order.status.load(Ordering::Relaxed);
            // Count statuses
            match status {
                OrderStatus::Pending => pending_count += 1,
                OrderStatus::Partial => partial_count += 1,
                OrderStatus::Filled => filled_count += 1,
                OrderStatus::Cancelled => cancelled_count += 1,
                OrderStatus::Expired => expired_count += 1,
            }
            // Count order types
            match order.order_type {
                OrderType::Limit => limit_count += 1,
                OrderType::Market => market_count += 1,
                OrderType::Stop => stop_count += 1,
                OrderType::StopLimit => stop_limit_count += 1,
                OrderType::IOC => ioc_count += 1,
                OrderType::FOK => fok_count += 1,
                OrderType::Iceberg => iceberg_count += 1,
                OrderType::DAY => day_count += 1,
                OrderType::GTC => gtc_count += 1,
            }
            total_filled += order.filled.load(Ordering::Relaxed);
            total_remaining += order.remaining.load(Ordering::Relaxed);
            if status == OrderStatus::Filled || status == OrderStatus::Partial {
                println!(
                    "{}: {:?} {:?} - Filled: {:.2}/Remaining: {:.2}",
                    id,
                    order.order_type,
                    order.direction,
                    order.filled.load(Ordering::Relaxed),
                    order.remaining.load(Ordering::Relaxed)
                );
            }
        }

        println!("\n=== STATUS DISTRIBUTION ===");
        println!("Pending: {}", pending_count);
        println!("Partial: {}", partial_count);
        println!("Filled: {}", filled_count);
        println!("Cancelled: {}", cancelled_count);
        println!("Expired: {}", expired_count);

        println!("\n=== ORDER TYPE DISTRIBUTION ===");
        println!("Limit: {}", limit_count);
        println!("Market: {}", market_count);
        println!("Stop: {}", stop_count);
        println!("StopLimit: {}", stop_limit_count);
        println!("IOC: {}", ioc_count);
        println!("FOK: {}", fok_count);
        println!("Iceberg: {}", iceberg_count);
        println!("DAY: {}", day_count);
        println!("GTC: {}", gtc_count);

        println!("\n=== VOLUME SUMMARY ===");
        println!("Total Filled: {:.2}", total_filled);
        println!("Total Remaining: {:.2}", total_remaining);
        let total_volume = total_filled + total_remaining;
        let fill_rate = if total_volume > 0.0 {
            (total_filled / total_volume * 100.0)
        } else {
            0.0
        };
        println!("Fill Rate: {:.1}%", fill_rate);
    }
}
