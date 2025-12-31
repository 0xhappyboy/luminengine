#[cfg(test)]
mod order_test_3 {
    use atomic_plus::AtomicF64;
    use luminengine::{
        matcher::{MatchEngine, MatchResult},
        orderbook::{
            OrderTree,
            order::{AtomicOrderStatus, Order, OrderDirection, OrderStatus, OrderType},
        },
    };
    use rand::Rng;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    };
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_continuous_order_book_display() {
        let run_duration = Duration::from_secs(30);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let engine = Arc::new(MatchEngine::new());
        let bids = Arc::new(OrderTree::new(true));
        let asks = Arc::new(OrderTree::new(false));
        let symbol = Arc::from("TEST");
        let trade_history: Arc<parking_lot::Mutex<Vec<TradeRecord>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        {
            let trade_history_clone = trade_history.clone();
            let engine_clone = engine.clone();
            engine_clone.set_match_callback(move |match_result: MatchResult| {
                let trade_record = TradeRecord {
                    timestamp: match_result.timestamp_ns,
                    buyer_id: match_result.buyer_order_id,
                    seller_id: match_result.seller_order_id,
                    price: match_result.price,
                    quantity: match_result.quantity,
                    is_cross: match_result.is_cross,
                };
                let mut history = trade_history_clone.lock();
                history.push(trade_record);
                if history.len() > 100 {
                    history.remove(0);
                }
            });
        }
        let display_engine = engine.clone();
        let display_bids = bids.clone();
        let display_asks = asks.clone();
        let display_trades = trade_history.clone();
        let display_running = running.clone();
        thread::spawn(move || {
            let mut last_display = std::time::Instant::now();
            let display_interval = Duration::from_millis(500);
            while display_running.load(Ordering::Relaxed) {
                if last_display.elapsed() >= display_interval {
                    print!("{}[2J", 27 as char);
                    print!("{}[H", 27 as char);
                    display_continuous_order_book(
                        &display_engine,
                        &display_bids,
                        &display_asks,
                        &display_trades.lock(),
                        10,
                    );
                    last_display = std::time::Instant::now();
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
        let order_engine = engine.clone();
        let order_bids = bids.clone();
        let order_asks = asks.clone();
        let order_running = running.clone();
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let mut order_counter = 0;
            while order_running.load(Ordering::Relaxed) {
                let should_add_order = rng.gen_bool(0.7);
                if should_add_order {
                    order_counter += 1;
                    let order_type = match rng.gen_range(0..=100) {
                        0..=60 => OrderType::Limit,
                        61..=75 => OrderType::Market,
                        76..=80 => OrderType::IOC,
                        81..=85 => OrderType::FOK,
                        86..=90 => OrderType::Stop,
                        91..=95 => OrderType::StopLimit,
                        96..=98 => OrderType::Iceberg,
                        99..=100 => OrderType::DAY,
                        _ => OrderType::Limit,
                    };
                    let direction = if rng.gen_bool(0.5) {
                        OrderDirection::Buy
                    } else {
                        OrderDirection::Sell
                    };
                    let base_price = 100.0;
                    let price_variation = rng.gen_range(-5.0..5.0);
                    let price = if order_type == OrderType::Market {
                        0.0
                    } else {
                        base_price + price_variation
                    };
                    let quantity = rng.gen_range(1.0..=50.0);
                    let order_id = format!(
                        "AUTO_{}_{}",
                        match direction {
                            OrderDirection::Buy => "B",
                            OrderDirection::Sell => "S",
                            OrderDirection::None => "N",
                        },
                        order_counter
                    );
                    let order = Arc::new(Order {
                        id: order_id.clone(),
                        symbol: "TEST".to_string(),
                        price: AtomicF64::new(price),
                        direction,
                        quantity: AtomicF64::new(quantity),
                        remaining: AtomicF64::new(quantity),
                        filled: AtomicF64::new(0.0),
                        crt_time: std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_millis()
                            .to_string(),
                        status: AtomicOrderStatus::new(OrderStatus::Pending),
                        expiry: if order_type == OrderType::DAY {
                            Some(std::time::Instant::now() + Duration::from_secs(300))
                        } else {
                            None
                        },
                        order_type,
                        ex: None,
                        version: AtomicU64::new(1),
                        timestamp_ns: std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos() as u64,
                        parent_order_id: None,
                        priority: rng.gen_range(1..=10),
                    });

                    let price_u64 = price as u64;
                    let is_bid = direction == OrderDirection::Buy;

                    if direction == OrderDirection::Buy {
                        order_bids.add_order(order.clone(), price_u64);
                    } else {
                        order_asks.add_order(order.clone(), price_u64);
                    }

                    if let Err(e) = order_engine.add_order(order.clone(), price_u64, is_bid) {
                        println!("Failed to add order {}: {}", order_id, e);
                    }
                }

                order_engine.execute_price_discovery(&order_bids, &order_asks, &symbol);

                let interval = rng.gen_range(100..500);
                thread::sleep(Duration::from_millis(interval));
            }
        });

        println!("🚀 Continuous order book simulation started!");
        println!("Running for {:?}...", run_duration);

        thread::sleep(run_duration);

        running.store(false, Ordering::Relaxed);

        print!("{}[2J", 27 as char);
        print!("{}[H", 27 as char);
        println!("=== FINAL STATE ===");
        display_continuous_order_book(&engine, &bids, &asks, &trade_history.lock(), 10);

        println!("\n✅ Continuous order book test completed!");
    }

    #[derive(Debug, Clone)]
    struct TradeRecord {
        timestamp: u64,
        buyer_id: String,
        seller_id: String,
        price: u64,
        quantity: u64,
        is_cross: bool,
    }

    fn display_continuous_order_book(
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        trade_history: &[TradeRecord],
        levels: usize,
    ) {
        let bid_levels = bids.get_price_levels(levels);
        let ask_levels = asks.get_price_levels(levels);
        let stats = engine.get_stats();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let recent_trades: Vec<&TradeRecord> = trade_history.iter().rev().take(10).collect();

        let max_rows = std::cmp::max(bid_levels.len(), ask_levels.len());
        let max_rows = std::cmp::max(max_rows, recent_trades.len());

        println!(
            "╔════════════════════════════════════════════════════════════════════════════════╗"
        );
        println!(
            "║                    REAL-TIME ORDER BOOK & TRADES ({})                     ║",
            chrono::Local::now().format("%H:%M:%S")
        );
        println!(
            "╠══════════════════════════════╦════════════════════════════════════════════════╣"
        );
        println!(
            "║        MARKET DEPTH          ║            RECENT TRADES                       ║"
        );
        println!(
            "╠════════════╦═════════════════╬════════════════════════════════════════════════╣"
        );
        println!(
            "║    BIDS    ║      ASKS       ║ Time  Price  Qty  Buyer←→Seller                ║"
        );
        println!(
            "╠════════════╬═════════════════╬════════════════════════════════════════════════╣"
        );

        for i in 0..max_rows {
            let (bid_display, ask_display) = if i < bid_levels.len() && i < ask_levels.len() {
                let bid_level = &bid_levels[bid_levels.len() - 1 - i];
                let ask_level = &ask_levels[i];
                (
                    format!("{:>4} {:>8}", bid_level.price, bid_level.quantity),
                    format!("{:>4} {:>8}", ask_level.price, ask_level.quantity),
                )
            } else if i < bid_levels.len() {
                let bid_level = &bid_levels[bid_levels.len() - 1 - i];
                (
                    format!("{:>4} {:>8}", bid_level.price, bid_level.quantity),
                    "              ".to_string(),
                )
            } else if i < ask_levels.len() {
                let ask_level = &ask_levels[i];
                (
                    "              ".to_string(),
                    format!("{:>4} {:>8}", ask_level.price, ask_level.quantity),
                )
            } else {
                ("              ".to_string(), "              ".to_string())
            };

            let trade_display = if i < recent_trades.len() {
                let trade = recent_trades[i];
                let time_since = now.saturating_sub(trade.timestamp / 1_000_000_000);
                let trade_type = if trade.is_cross { "CROSS" } else { "NORM" };
                format!(
                    "{:>4}s {:>5} {:>4} {:>8}←{:>8} ({})",
                    time_since,
                    trade.price,
                    trade.quantity,
                    &trade.buyer_id[..std::cmp::min(8, trade.buyer_id.len())],
                    &trade.seller_id[..std::cmp::min(8, trade.seller_id.len())],
                    trade_type
                )
            } else {
                "                                                ".to_string()
            };

            println!(
                "║ {:^12} ║ {:^15} ║ {} ║",
                bid_display, ask_display, trade_display
            );
        }

        println!(
            "╠════════════╩═════════════════╬════════════════════════════════════════════════╣"
        );
        let best_bid = bid_levels.first().map(|l| l.price).unwrap_or(0);
        let best_ask = ask_levels.first().map(|l| l.price).unwrap_or(0);
        let spread = if best_bid > 0 && best_ask > 0 {
            best_ask.saturating_sub(best_bid)
        } else {
            0
        };
        let active_bids = bids.size.load(Ordering::Relaxed);
        let active_asks = asks.size.load(Ordering::Relaxed);
        println!(
            "║ Best: {:>4} Spread: {:>3}  ║ Matches: {:>6} Vol: {:>10}              ║",
            best_bid,
            spread,
            stats.total_matches.load(Ordering::Relaxed),
            stats.total_volume.load(Ordering::Relaxed)
        );
        println!(
            "║ Active: B{:>3}/S{:>3}        ║ Notional: {:>10} Cross: {:>6}        ║",
            active_bids,
            active_asks,
            stats.total_notional.load(Ordering::Relaxed),
            stats.cross_matches.load(Ordering::Relaxed)
        );
        println!(
            "╚══════════════════════════════╩════════════════════════════════════════════════╝"
        );
        if best_bid >= best_ask && best_bid > 0 && best_ask > 0 {
            println!(
                "⚠️  WARNING: CROSS MARKET DETECTED! BID: {} >= ASK: {}",
                best_bid, best_ask
            );
        }
        let match_rate = if stats.orders_processed.load(Ordering::Relaxed) > 0 {
            stats.total_matches.load(Ordering::Relaxed) as f64
                / stats.orders_processed.load(Ordering::Relaxed) as f64
                * 100.0
        } else {
            0.0
        };
        println!(
            "Market Status: {:.1}% match rate | {:.1}ms avg latency",
            match_rate,
            stats.match_latency_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0
        );
    }

    #[allow(dead_code)]
    fn run_forever_simulation() {
        let engine = Arc::new(MatchEngine::new());
        let bids = Arc::new(OrderTree::new(true));
        let asks = Arc::new(OrderTree::new(false));
        let symbol: Arc<str> = Arc::from("LIVE");
        println!("🚀 Starting live order book simulation...");
        println!("Press Ctrl+C to exit");
        println!();
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
}
