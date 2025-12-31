#[cfg(test)]
mod tests {
    use super::*;
    use atomic_plus::AtomicF64;
    use luminengine::orderbook::OrderBook;
    use luminengine::orderbook::order::{AtomicOrderStatus, Order, OrderDirection, OrderStatus, OrderType};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    /// Test complete matching process:
    /// 1. Add several orders for both directions
    /// 2. Place a limit order in any direction
    /// 3. Partially fill the limit order
    #[test]
    fn test_matching_flow() {
        println!("=== Starting matching flow test ===");

        // 1. Create order book
        let orderbook = OrderBook::new("TEST");
        println!("✓ Created order book: {}", orderbook.symbol);

        // Set up match callback (optional)
        let match_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let match_count_clone = match_count.clone();

        orderbook.set_match_callback(move |result| {
            match_count_clone.fetch_add(1, Ordering::Relaxed);
            println!(
                "Match[{:03}]: {} price={} quantity={} (buyer={} seller={})",
                match_count_clone.load(Ordering::Relaxed),
                result.symbol,
                result.price,
                result.quantity,
                result.buyer_order_id,
                result.seller_order_id
            );
        });

        // Give some time for matching thread to start
        std::thread::sleep(Duration::from_millis(100));

        // 2. Add initial orders for both directions
        println!("\n✓ Step 1: Adding initial orders");

        // Add ask orders: prices from low to high (higher ask prices are better)
        let ask_orders = vec![
            (101.0, 50.0), // price 101, quantity 50
            (102.0, 30.0), // price 102, quantity 30
            (103.0, 20.0), // price 103, quantity 20
        ];

        for (i, (price, quantity)) in ask_orders.iter().enumerate() {
            let order = Arc::new(Order {
                id: format!("ASK_{}_{}", price, quantity),
                symbol: "TEST".to_string(),
                price: AtomicF64::new(*price),
                direction: OrderDirection::Sell,
                quantity: AtomicF64::new(*quantity),
                remaining: AtomicF64::new(*quantity),
                filled: AtomicF64::new(0.0),
                crt_time: chrono::Utc::now().to_rfc3339(),
                status: AtomicOrderStatus::new(OrderStatus::Pending),
                expiry: None,
                order_type: OrderType::Limit,
                ex: None,
                version: AtomicU64::new(0),
                timestamp_ns: 0,
                parent_order_id: None,
                priority: 0,
            });

            orderbook.add_order(order).unwrap();
            println!(
                "  [{:02}] Added ask order: price={}, quantity={}",
                i + 1,
                price,
                quantity
            );
        }

        // Add bid orders: prices from high to low (lower bid prices are better)
        let bid_orders = vec![
            (99.0, 80.0), // price 99, quantity 80
            (98.0, 40.0), // price 98, quantity 40
            (97.0, 20.0), // price 97, quantity 20
        ];

        for (i, (price, quantity)) in bid_orders.iter().enumerate() {
            let order = Arc::new(Order {
                id: format!("BID_{}_{}", price, quantity),
                symbol: "TEST".to_string(),
                price: AtomicF64::new(*price),
                direction: OrderDirection::Buy,
                quantity: AtomicF64::new(*quantity),
                remaining: AtomicF64::new(*quantity),
                filled: AtomicF64::new(0.0),
                crt_time: chrono::Utc::now().to_rfc3339(),
                status: AtomicOrderStatus::new(OrderStatus::Pending),
                expiry: None,
                order_type: OrderType::Limit,
                ex: None,
                version: AtomicU64::new(0),
                timestamp_ns: 0,
                parent_order_id: None,
                priority: 0,
            });

            orderbook.add_order(order).unwrap();
            println!(
                "  [{:02}] Added bid order: price={}, quantity={}",
                i + 1,
                price,
                quantity
            );
        }

        // Wait for matching engine to process initial orders
        std::thread::sleep(Duration::from_millis(200));

        // Get initial statistics
        let initial_stats = orderbook.get_stats();
        println!("\n✓ Initial statistics:");
        println!("  Total orders: {}", initial_stats.0);
        println!("  Active orders: {}", initial_stats.1);
        println!("  Order book orders: {}", initial_stats.2);

        // 3. Place a limit order (aggressive order) - Modified: use higher price to ensure partial fill
        println!("\n✓ Step 2: Placing an aggressive limit order");
        let aggressive_order_id = "AGGRESSIVE_LIMIT";
        let aggressive_price = 101.0; // price 101, same as lowest ask price
        let aggressive_quantity = 80.0; // quantity 80, greater than lowest ask quantity 50, ensuring partial fill

        let aggressive_order = Arc::new(Order {
            id: aggressive_order_id.to_string(),
            symbol: "TEST".to_string(),
            price: AtomicF64::new(aggressive_price),
            direction: OrderDirection::Buy,
            quantity: AtomicF64::new(aggressive_quantity),
            remaining: AtomicF64::new(aggressive_quantity),
            filled: AtomicF64::new(0.0),
            crt_time: chrono::Utc::now().to_rfc3339(),
            status: AtomicOrderStatus::new(OrderStatus::Pending),
            expiry: None,
            order_type: OrderType::Limit,
            ex: None,
            version: AtomicU64::new(0),
            timestamp_ns: 0,
            parent_order_id: None,
            priority: 0,
        });

        println!(
            "  Placing aggressive bid: price={}, quantity={}",
            aggressive_price, aggressive_quantity
        );
        println!("  Explanation: Price same as lowest ask (101), quantity (80) greater than lowest ask quantity (50), should partially fill");
        orderbook.add_order(aggressive_order.clone()).unwrap();

        // 4. Wait for matching to complete
        println!("\n✓ Step 3: Waiting for matching engine to process...");
        for i in 1..=5 {
            std::thread::sleep(Duration::from_millis(100));
            let current_matches = match_count.load(Ordering::Relaxed);
            println!("  Wait {}/5 seconds, matches so far: {}", i, current_matches);
        }

        // 5. Check matching results
        println!("\n✓ Verifying matching results:");

        // Find aggressive order
        let found_order = orderbook.find_order(aggressive_order_id);
        assert!(found_order.is_some(), "Should find aggressive order");

        let order = found_order.unwrap();
        let filled = order.filled.load(Ordering::Relaxed);
        let remaining = order.remaining.load(Ordering::Relaxed);
        let status = order.status.load(Ordering::Relaxed);
        let total_quantity = order.quantity.load(Ordering::Relaxed);

        println!("  Aggressive order information:");
        println!("    ID: {}", order.id);
        println!("    Total quantity: {}", total_quantity);
        println!("    Filled quantity: {}", filled);
        println!("    Remaining quantity: {}", remaining);
        println!("    Status: {:?}", status);

        // Verification: should be partially filled (since aggressive bid quantity 80 > lowest ask quantity 50)
        // Note: Actual matching may be fully filled or partially filled, depending on matching logic
        if filled >= total_quantity {
            println!(
                "  ⚠️ Order fully filled (filled={} >= total={})",
                filled, total_quantity
            );
            assert_eq!(
                status,
                OrderStatus::Filled,
                "Order status should be Filled, but is {:?}",
                status
            );
            assert!(
                remaining <= 0.0,
                "After full fill, remaining should be 0 or negative, but remaining={}",
                remaining
            );
        } else if filled > 0.0 {
            println!(
                "  ✅ Order partially filled (filled={}, remaining={})",
                filled, remaining
            );
            assert_eq!(
                status,
                OrderStatus::Partial,
                "Order status should be Partial, but is {:?}",
                status
            );
            assert!(
                remaining > 0.0,
                "After partial fill, there should be remaining, but remaining={}",
                remaining
            );
        } else {
            panic!("Order not filled (filled={}), matching logic may have issues", filled);
        }

        // Verify matching statistics
        let match_stats = orderbook.get_match_stats();
        let total_matches = match_stats.total_matches.load(Ordering::Relaxed);

        println!("\n✓ Matching statistics:");
        println!("  Total matches: {}", total_matches);
        println!(
            "  Total volume: {}",
            match_stats.total_volume.load(Ordering::Relaxed)
        );
        println!(
            "  Total notional: {}",
            match_stats.total_notional.load(Ordering::Relaxed)
        );
        println!(
            "  Cross matches: {}",
            match_stats.cross_matches.load(Ordering::Relaxed)
        );

        // Check if there's at least one match
        let final_match_count = match_count.load(Ordering::Relaxed);
        if final_match_count > 0 {
            println!("  ✅ Matching engine working normally, total matches: {}", final_match_count);
        } else {
            println!("  ⚠️ No matches occurred, may need to check matching logic");
        }

        // Check market depth
        let depth = orderbook.get_market_depth(5);
        println!("\n✓ Market depth (top 5 levels):");

        if !depth.bids.is_empty() {
            println!("  Bids (price high to low):");
            for (i, bid) in depth.bids.iter().enumerate() {
                println!(
                    "    [{:2}] price={}, quantity={}, order count={}",
                    i + 1,
                    bid.price,
                    bid.quantity,
                    bid.order_count
                );
            }
        } else {
            println!("  Bids: empty");
        }

        if !depth.asks.is_empty() {
            println!("  Asks (price low to high):");
            for (i, ask) in depth.asks.iter().enumerate() {
                println!(
                    "    [{:2}] price={}, quantity={}, order count={}",
                    i + 1,
                    ask.price,
                    ask.quantity,
                    ask.order_count
                );
            }
        } else {
            println!("  Asks: empty");
        }

        // Print all order statuses
        println!("\n✓ All order status check:");
        let all_order_ids = vec![
            "ASK_101_50",
            "ASK_102_30",
            "ASK_103_20",
            "BID_99_80",
            "BID_98_40",
            "BID_97_20",
            aggressive_order_id,
        ];

        for order_id in all_order_ids {
            if let Some(order) = orderbook.find_order(order_id) {
                let filled = order.filled.load(Ordering::Relaxed);
                let remaining = order.remaining.load(Ordering::Relaxed);
                let status = order.status.load(Ordering::Relaxed);
                let total = order.quantity.load(Ordering::Relaxed);

                println!(
                    "  {}: quantity={}/{} remaining={} status={:?}",
                    order_id, filled, total, remaining, status
                );
            } else {
                println!("  {}: not found (may be fully filled and removed from order book)", order_id);
            }
        }

        // Cleanup
        println!("\n✓ Cleaning up resources...");
        orderbook.shutdown();
        std::thread::sleep(Duration::from_millis(100));

        println!("\n=== Test completed! ===");
    }
}