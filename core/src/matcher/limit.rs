use super::OrderHandler;
use crate::{
    matcher::{MatchEngine, OrderMatchContext},
    orderbook::OrderTree,
    orderbook::order::{Order, OrderDirection, OrderStatus, OrderType},
};
use crossbeam::epoch;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

pub struct LimitOrderHandler;

impl OrderHandler for LimitOrderHandler {
    fn add_order(
        &self,
        order: Arc<Order>,
        price: u64,
        is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String> {
        // Limit order: add to pending queue
        let ctx = OrderMatchContext::new(order.clone(), price, is_bid);
        engine.pending_limit_orders.lock().push_back(ctx);
        // Also add to price level matcher
        engine.add_to_price_matcher(&order, price, is_bid);
        Ok(())
    }

    fn process_matching(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        self.process_limit_orders(engine, bids, asks, symbol)
    }

    fn get_order_type(&self) -> OrderType {
        OrderType::Limit
    }
}

impl LimitOrderHandler {
    /// Process limit order matching logic (migrated from MatchEngine)
    fn process_limit_orders(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        let mut pending_orders = engine.pending_limit_orders.lock();
        let mut processed_indices = Vec::new();
        for (idx, ctx) in pending_orders.iter_mut().enumerate() {
            // Check if the order can still be matched
            let order = &ctx.order;
            let remaining = order.remaining.load(Ordering::Relaxed);
            if remaining <= 0.0 {
                processed_indices.push(idx);
                continue;
            }
            // Check order status
            let status = order.status.load(Ordering::Relaxed);
            if status == OrderStatus::Cancelled || status == OrderStatus::Expired {
                processed_indices.push(idx);
                continue;
            }
            // Find opposite side
            let opposite_tree = if ctx.is_bid { asks } else { bids };
            let guard = &epoch::pin();
            let levels = opposite_tree.get_price_levels(10);
            for level in levels {
                let opposite_price = level.price as u64;
                // Check if price is matchable
                let price_ok = if ctx.is_bid {
                    // Buy order: buy price >= sell price
                    ctx.price >= opposite_price
                } else {
                    // Sell order: sell price <= buy price
                    ctx.price <= opposite_price
                };
                if !price_ok {
                    continue;
                }
                // Get opposite orders
                let orders = self.get_orders_at_price(opposite_tree, opposite_price, guard);
                for opposite_order in orders {
                    if remaining <= 0.0 {
                        break;
                    }
                    let opposite_remaining = opposite_order.remaining.load(Ordering::Relaxed);
                    if opposite_remaining <= 0.0 {
                        continue;
                    }
                    let match_qty = remaining.min(opposite_remaining);
                    if match_qty > 0.0 {
                        // Determine match price (passive side price priority)
                        let match_price = if ctx.is_bid {
                            // Buy order active, trade at seller's price
                            opposite_price
                        } else {
                            // Sell order active, trade at buyer's price
                            opposite_price
                        };
                        // Execute match
                        self.execute_match(
                            engine,
                            if ctx.is_bid { order } else { &opposite_order },
                            if ctx.is_bid { &opposite_order } else { order },
                            match_price,
                            match_qty,
                            symbol,
                            ctx.price,
                            opposite_price,
                            false,
                        );
                        // Update remaining quantity
                        let new_remaining = remaining - match_qty;
                        order.remaining.store(new_remaining, Ordering::SeqCst);
                        if new_remaining <= 0.0 {
                            order.status.store(OrderStatus::Filled, Ordering::SeqCst);
                        } else {
                            order.status.store(OrderStatus::Partial, Ordering::SeqCst);
                        }
                        ctx.match_attempts += 1;
                        ctx.last_match_time = Instant::now();
                        // Update statistics
                        engine.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                        engine
                            .stats
                            .total_volume
                            .fetch_add(match_qty as u64, Ordering::Relaxed);
                        engine
                            .stats
                            .total_notional
                            .fetch_add((match_qty * match_price as f64) as u64, Ordering::Relaxed);
                    }
                }
            }
            // If order is fully filled, mark as processed
            if order.remaining.load(Ordering::Relaxed) <= 0.0 {
                processed_indices.push(idx);
            }
        }
        // Remove processed orders (remove from back to avoid index confusion)
        for &idx in processed_indices.iter().rev() {
            if idx < pending_orders.len() {
                pending_orders.remove(idx);
            }
        }
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

    /// Execute a single match
    pub fn execute_match(
        &self,
        engine: &MatchEngine,
        aggressive: &Arc<Order>,
        passive: &Arc<Order>,
        price: u64,
        quantity: f64,
        symbol: &Arc<str>,
        bid_price: u64,
        ask_price: u64,
        is_cross: bool,
    ) {
        // Generate match ID
        let match_id = engine.match_id_counter.fetch_add(1, Ordering::SeqCst);
        // Create match result
        let match_result = crate::matcher::MatchResult::new(
            match_id,
            symbol.clone(),
            if aggressive.direction == OrderDirection::Buy {
                aggressive.id.clone()
            } else {
                passive.id.clone()
            },
            if aggressive.direction == OrderDirection::Sell {
                aggressive.id.clone()
            } else {
                passive.id.clone()
            },
            price,
            quantity as u64,
            bid_price,
            ask_price,
            is_cross,
        );
        // Update order status
        aggressive.filled.fetch_add(quantity, Ordering::SeqCst);
        aggressive.remaining.fetch_sub(quantity, Ordering::SeqCst);
        passive.filled.fetch_add(quantity, Ordering::SeqCst);
        passive.remaining.fetch_sub(quantity, Ordering::SeqCst);
        // Check if orders are fully filled
        if aggressive.remaining.load(Ordering::Relaxed) <= 0.0 {
            aggressive
                .status
                .store(OrderStatus::Filled, Ordering::SeqCst);
        } else {
            aggressive
                .status
                .store(OrderStatus::Partial, Ordering::SeqCst);
        }
        if passive.remaining.load(Ordering::Relaxed) <= 0.0 {
            passive.status.store(OrderStatus::Filled, Ordering::SeqCst);
        } else {
            passive.status.store(OrderStatus::Partial, Ordering::SeqCst);
        }
        // Call match result callback (if set)
        if let Some(callback) = engine.match_callback.lock().as_ref() {
            callback(match_result);
        }
    }

    /// Execute cross-market matching (migrated from MatchEngine)
    pub fn execute_cross_matching(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
        best_bid: u64,
        best_ask: u64,
    ) {
        // Match price: take mid-price or better price
        let match_price = if best_bid == best_ask {
            best_bid
        } else {
            // Price improvement: take price more favorable to active side
            // Simplified to mid-price here, actual exchange rules differ
            (best_bid + best_ask) / 2
        };
        let guard = &epoch::pin();
        // Get orders from both sides at cross prices
        let bid_orders = self.get_orders_at_price(bids, best_bid, guard);
        let ask_orders = self.get_orders_at_price(asks, best_ask, guard);
        // Match according to price-time priority
        let mut bid_idx = 0;
        let mut ask_idx = 0;
        while bid_idx < bid_orders.len() && ask_idx < ask_orders.len() {
            let bid_order = &bid_orders[bid_idx];
            let ask_order = &ask_orders[ask_idx];
            let bid_remaining = bid_order.remaining.load(Ordering::Relaxed);
            let ask_remaining = ask_order.remaining.load(Ordering::Relaxed);
            // Skip fully filled orders
            if bid_remaining <= 0.0 {
                bid_idx += 1;
                continue;
            }
            if ask_remaining <= 0.0 {
                ask_idx += 1;
                continue;
            }
            // Calculate matchable quantity
            let match_qty = bid_remaining.min(ask_remaining);
            if match_qty > 0.0 {
                // Execute match
                self.execute_match(
                    engine,
                    bid_order,
                    ask_order,
                    match_price,
                    match_qty,
                    symbol,
                    best_bid,
                    best_ask,
                    true, // is_cross
                );
                // Update statistics
                engine.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                engine
                    .stats
                    .total_volume
                    .fetch_add(match_qty as u64, Ordering::Relaxed);
                engine
                    .stats
                    .total_notional
                    .fetch_add((match_qty * match_price as f64) as u64, Ordering::Relaxed);
            }
            // Move to next order
            if bid_remaining <= ask_remaining {
                bid_idx += 1;
            }
            if ask_remaining <= bid_remaining {
                ask_idx += 1;
            }
        }
    }
}
