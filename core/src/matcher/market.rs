use super::OrderHandler;
use crate::{
    matcher::{LimitOrderHandler, MatchEngine},
    orderbook::{
        OrderTree,
        order::{Order, OrderDirection, OrderStatus, OrderType},
    },
};
use crossbeam::epoch;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct MarketOrderHandler;

impl OrderHandler for MarketOrderHandler {
    fn add_order(
        &self,
        order: Arc<Order>,
        _price: u64,
        _is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String> {
        // Market order: process immediately
        engine.market_order_queue.lock().push_back(order);
        Ok(())
    }

    fn process_matching(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        self.process_market_orders(engine, bids, asks, symbol)
    }

    fn get_order_type(&self) -> OrderType {
        OrderType::Market
    }
}

impl MarketOrderHandler {
    /// Process market order matching logic (migrated from MatchEngine)
    fn process_market_orders(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        let mut market_orders = engine.market_order_queue.lock();
        let mut processed_orders = Vec::new();
        for market_order in market_orders.iter() {
            // Market order matches with best counterparty
            let opposite_tree = match market_order.direction {
                OrderDirection::Buy => asks,
                OrderDirection::Sell => bids,
                OrderDirection::None => {
                    processed_orders.push(market_order.clone());
                    continue;
                }
            };
            let guard = &epoch::pin();
            let levels = opposite_tree.get_price_levels(10);
            if levels.is_empty() {
                // No counterparty, market order cannot be filled
                market_order
                    .status
                    .store(OrderStatus::Cancelled, Ordering::SeqCst);
                processed_orders.push(market_order.clone());
                continue;
            }
            // Start matching from best price
            let mut remaining_qty = market_order.remaining.load(Ordering::Relaxed);
            for level in levels {
                if remaining_qty <= 0.0 {
                    break;
                }
                let price = level.price as u64;
                // Get orders at this price level
                let orders = self.get_orders_at_price(opposite_tree, price, guard);
                for order in orders {
                    if remaining_qty <= 0.0 {
                        break;
                    }
                    let order_remaining = order.remaining.load(Ordering::Relaxed);
                    if order_remaining <= 0.0 {
                        continue;
                    }
                    let match_qty = remaining_qty.min(order_remaining);
                    if match_qty > 0.0 {
                        // Determine match parties
                        let (aggressive, passive) = match market_order.direction {
                            OrderDirection::Buy => (market_order, &order),
                            OrderDirection::Sell => (&order, market_order),
                            _ => continue,
                        };
                        // Execute match
                        LimitOrderHandler.execute_match(
                            engine, aggressive, passive, price, match_qty, symbol, price, price,
                            false,
                        );
                        remaining_qty -= match_qty;
                        // Update statistics
                        engine.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                        engine
                            .stats
                            .total_volume
                            .fetch_add(match_qty as u64, Ordering::Relaxed);
                        engine
                            .stats
                            .total_notional
                            .fetch_add((match_qty * price as f64) as u64, Ordering::Relaxed);
                    }
                }
            }
            // Update market order remaining quantity
            market_order
                .remaining
                .store(remaining_qty, Ordering::SeqCst);
            if remaining_qty <= 0.0 {
                market_order
                    .status
                    .store(OrderStatus::Filled, Ordering::SeqCst);
            } else {
                // Partially filled
                market_order
                    .status
                    .store(OrderStatus::Partial, Ordering::SeqCst);
            }
            processed_orders.push(market_order.clone());
        }
        // Remove processed orders
        market_orders.retain(|order| {
            !processed_orders
                .iter()
                .any(|processed_order| processed_order.id == order.id)
        });
        Ok(())
    }

    /// Get all orders at specified price
    fn get_orders_at_price(
        &self,
        tree: &OrderTree,
        price: u64,
        guard: &epoch::Guard,
    ) -> Vec<Arc<Order>> {
        let mut orders = Vec::new();
        let mut current = tree.head.load(Ordering::Relaxed, guard);
        while let Some(node) = unsafe { current.as_ref() } {
            if node.price == price {
                if let Some(order_queue) =
                    unsafe { node.orders.load(Ordering::Relaxed, guard).as_ref() }
                {
                    let mut order_node = order_queue.head.load(Ordering::Relaxed, guard);
                    while let Some(node_ref) = unsafe { order_node.as_ref() } {
                        orders.push(node_ref.order.clone());
                        order_node = node_ref.next.load(Ordering::Relaxed, guard);
                    }
                }
                break;
            }
            current = node.forward[0].load(Ordering::Relaxed, guard);
        }
        orders
    }
}
