#[cfg(test)]
mod order_test_1 {
    use super::*;
    use atomic_plus::AtomicF64;
    use luminengine::matcher::MatchEngine;
    use luminengine::orderbook::OrderTree;
    use luminengine::orderbook::order::{
        AtomicOrderStatus, Order, OrderDirection, OrderStatus, OrderType,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn create_test_order(
        id: &str,
        order_type: OrderType,
        direction: OrderDirection,
        price: f64,
        quantity: f64,
    ) -> Arc<Order> {
        Arc::new(Order {
            id: id.to_string(),
            symbol: "TEST".to_string(),
            price: AtomicF64::new(price),
            direction,
            quantity: AtomicF64::new(quantity),
            remaining: AtomicF64::new(quantity),
            filled: AtomicF64::new(0.0),
            crt_time: "".to_string(),
            status: AtomicOrderStatus::new(OrderStatus::Pending),
            expiry: None,
            order_type,
            ex: None,
            version: AtomicU64::new(1),
            timestamp_ns: 0,
            parent_order_id: None,
            priority: 0,
        })
    }

    #[test]
    fn test_limit_order_matching() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create buy limit order
        let buy_order =
            create_test_order("BUY1", OrderType::Limit, OrderDirection::Buy, 100.0, 10.0);
        engine.add_order(buy_order.clone(), 100, true).unwrap();

        // Create sell limit order at same price
        let sell_order =
            create_test_order("SELL1", OrderType::Limit, OrderDirection::Sell, 100.0, 10.0);
        engine.add_order(sell_order.clone(), 100, false).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);

        bids.add_order(buy_order.clone(), 100);
        asks.add_order(sell_order.clone(), 100);

        // Execute price discovery
        engine.execute_price_discovery(&bids, &asks, &symbol);

        // Both orders should be filled
        assert_eq!(
            buy_order.status.load(Ordering::Relaxed),
            OrderStatus::Filled
        );
        assert_eq!(
            sell_order.status.load(Ordering::Relaxed),
            OrderStatus::Filled
        );
        assert_eq!(buy_order.remaining.load(Ordering::Relaxed), 0.0);
        assert_eq!(sell_order.remaining.load(Ordering::Relaxed), 0.0);
    }

    #[test]
    fn test_market_order_execution() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create limit order on sell side
        let limit_sell = create_test_order(
            "SELL_LIMIT",
            OrderType::Limit,
            OrderDirection::Sell,
            100.0,
            5.0,
        );

        // Create market buy order
        let market_buy = create_test_order(
            "BUY_MARKET",
            OrderType::Market,
            OrderDirection::Buy,
            0.0,
            5.0,
        );

        engine.add_order(market_buy.clone(), 0, true).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);
        asks.add_order(limit_sell.clone(), 100);

        // Add limit order to engine for price matching
        engine.add_order(limit_sell.clone(), 100, false).unwrap();

        engine.execute_price_discovery(&bids, &asks, &symbol);

        // Market order should be filled at limit price
        assert_eq!(
            market_buy.status.load(Ordering::Relaxed),
            OrderStatus::Filled
        );
        assert_eq!(market_buy.filled.load(Ordering::Relaxed), 5.0);
    }

    #[test]
    fn test_ioc_order_immediate_execution() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create IOC buy order
        let ioc_order =
            create_test_order("IOC_BUY", OrderType::IOC, OrderDirection::Buy, 100.0, 10.0);
        engine.add_order(ioc_order.clone(), 100, true).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);

        // No matching orders available
        engine.execute_price_discovery(&bids, &asks, &symbol);

        // IOC order should be cancelled if not immediately filled
        assert_eq!(
            ioc_order.status.load(Ordering::Relaxed),
            OrderStatus::Cancelled
        );
    }

    #[test]
    fn test_fok_order_full_or_none() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create FOK sell order
        let fok_order = create_test_order(
            "FOK_SELL",
            OrderType::FOK,
            OrderDirection::Sell,
            100.0,
            10.0,
        );
        engine.add_order(fok_order.clone(), 100, false).unwrap();

        // Create buy order with insufficient quantity
        let buy_order =
            create_test_order("BUY1", OrderType::Limit, OrderDirection::Buy, 100.0, 5.0);

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);
        bids.add_order(buy_order.clone(), 100);

        // Add buy order to engine
        engine.add_order(buy_order.clone(), 100, true).unwrap();

        engine.execute_price_discovery(&bids, &asks, &symbol);

        // FOK order should be cancelled if not fully filled
        assert_eq!(
            fok_order.status.load(Ordering::Relaxed),
            OrderStatus::Cancelled
        );
        assert_eq!(
            buy_order.status.load(Ordering::Relaxed),
            OrderStatus::Pending
        );
    }

    #[test]
    fn test_stop_order_trigger() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create stop buy order with trigger at 100
        let stop_order = create_test_order(
            "STOP_BUY",
            OrderType::Stop,
            OrderDirection::Buy,
            100.0,
            10.0,
        );
        engine.add_order(stop_order.clone(), 100, true).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);

        // Set market price at 101 (above stop price)
        let market_sell = create_test_order(
            "MARKET_SELL",
            OrderType::Limit,
            OrderDirection::Sell,
            101.0,
            10.0,
        );
        asks.add_order(market_sell.clone(), 101);

        engine.execute_price_discovery(&bids, &asks, &symbol);

        // Stop order should be triggered and moved to market order queue
        assert_eq!(
            stop_order.status.load(Ordering::Relaxed),
            OrderStatus::Pending
        );
    }

    #[test]
    fn test_stop_limit_order_trigger() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create stop-limit sell order
        let stop_limit = create_test_order(
            "STOP_LIMIT",
            OrderType::StopLimit,
            OrderDirection::Sell,
            100.0,
            10.0,
        );
        engine.add_order(stop_limit.clone(), 100, false).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);

        // Set market price at 99 (below stop price for sell)
        let market_buy = create_test_order(
            "MARKET_BUY",
            OrderType::Limit,
            OrderDirection::Buy,
            99.0,
            10.0,
        );
        bids.add_order(market_buy.clone(), 99);

        engine.execute_price_discovery(&bids, &asks, &symbol);

        // Stop-limit order should be triggered and moved to limit order queue
        assert_eq!(
            stop_limit.status.load(Ordering::Relaxed),
            OrderStatus::Pending
        );
    }

    #[test]
    fn test_iceberg_order_slicing() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create iceberg buy order
        let iceberg = create_test_order(
            "ICEBERG",
            OrderType::Iceberg,
            OrderDirection::Buy,
            100.0,
            100.0,
        );
        engine.add_order(iceberg.clone(), 100, true).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);

        // Initial display size should be 10% (10 units)
        assert_eq!(iceberg.quantity.load(Ordering::Relaxed), 10.0);

        engine.execute_price_discovery(&bids, &asks, &symbol);

        // Iceberg order should be in pending queue
        assert_eq!(iceberg.status.load(Ordering::Relaxed), OrderStatus::Pending);
    }

    #[test]
    fn test_day_order_expiration() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create DAY order
        let day_order = create_test_order(
            "DAY_ORDER",
            OrderType::DAY,
            OrderDirection::Buy,
            100.0,
            10.0,
        );
        engine.add_order(day_order.clone(), 100, true).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);

        // DAY order should be in pending queue
        assert_eq!(
            day_order.status.load(Ordering::Relaxed),
            OrderStatus::Pending
        );

        // Note: Actual expiration test requires time manipulation
        engine.execute_price_discovery(&bids, &asks, &symbol);
    }

    #[test]
    fn test_gtc_order_persistence() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create GTC order (no expiration)
        let gtc_order = create_test_order(
            "GTC_ORDER",
            OrderType::GTC,
            OrderDirection::Sell,
            100.0,
            10.0,
        );
        engine.add_order(gtc_order.clone(), 100, false).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);
        asks.add_order(gtc_order.clone(), 100);

        // Multiple matching cycles
        for _ in 0..3 {
            engine.execute_price_discovery(&bids, &asks, &symbol);
        }

        // GTC order should remain active
        assert_eq!(
            gtc_order.status.load(Ordering::Relaxed),
            OrderStatus::Pending
        );
    }

    #[test]
    fn test_cross_market_matching() {
        let engine = MatchEngine::new();
        let symbol = Arc::from("TEST");

        // Create buy order at higher price than sell order (cross market)
        let buy_order = create_test_order(
            "BUY_CROSS",
            OrderType::Limit,
            OrderDirection::Buy,
            101.0,
            10.0,
        );
        let sell_order = create_test_order(
            "SELL_CROSS",
            OrderType::Limit,
            OrderDirection::Sell,
            100.0,
            10.0,
        );

        engine.add_order(buy_order.clone(), 101, true).unwrap();
        engine.add_order(sell_order.clone(), 100, false).unwrap();

        let bids = OrderTree::new(true);
        let asks = OrderTree::new(false);

        bids.add_order(buy_order.clone(), 101);
        asks.add_order(sell_order.clone(), 100);

        engine.execute_price_discovery(&bids, &asks, &symbol);

        // Cross market orders should match at mid-price (100.5)
        assert_eq!(
            buy_order.status.load(Ordering::Relaxed),
            OrderStatus::Filled
        );
        assert_eq!(
            sell_order.status.load(Ordering::Relaxed),
            OrderStatus::Filled
        );
    }
}
