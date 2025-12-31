use super::OrderHandler;
use crate::{
    matcher::MatchEngine,
    matcher::limit::LimitOrderHandler,
    orderbook::OrderTree,
    orderbook::order::{Order, OrderDirection, OrderStatus, OrderType},
};
use crossbeam::epoch;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct ImmediateOrderHandler;

impl OrderHandler for ImmediateOrderHandler {
    fn add_order(
        &self,
        order: Arc<Order>,
        _price: u64,
        _is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String> {
        // Immediate or cancel order: process immediately
        engine.immediate_order_queue.lock().push(order);
        Ok(())
    }

    fn process_matching(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        self.process_immediate_orders(engine, bids, asks, symbol)
    }

    fn get_order_type(&self) -> OrderType {
        OrderType::IOC
    }
}

impl ImmediateOrderHandler {
    fn process_immediate_orders(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        let mut immediate_orders = engine.immediate_order_queue.lock();
        let mut indices_to_remove = Vec::new();
        for (index, order) in immediate_orders.iter().enumerate() {
            let remaining = order.remaining.load(Ordering::Relaxed);
            if remaining <= 0.0 {
                indices_to_remove.push(index);
                continue;
            }
            let opposite_tree = match order.direction {
                OrderDirection::Buy => asks,
                OrderDirection::Sell => bids,
                OrderDirection::None => {
                    order.status.store(OrderStatus::Cancelled, Ordering::SeqCst);
                    indices_to_remove.push(index);
                    continue;
                }
            };
            let guard = &epoch::pin();
            let levels = opposite_tree.get_price_levels(10);
            let order_price = order.price.load(Ordering::Relaxed) as u64;
            let mut matched_qty = 0.0;
            let mut total_available = 0.0;
            for level in &levels {
                let level_price = level.price as u64;
                let price_ok = match order.direction {
                    OrderDirection::Buy => order_price >= level_price,
                    OrderDirection::Sell => order_price <= level_price,
                    _ => false,
                };
                if price_ok {
                    total_available += level.quantity as f64;
                }
            }
            if order.order_type == OrderType::FOK && total_available < remaining {
                order.status.store(OrderStatus::Cancelled, Ordering::SeqCst);
                indices_to_remove.push(index);
                continue;
            }
            for level in levels {
                if matched_qty >= remaining {
                    break;
                }
                let level_price = level.price as u64;
                let price_ok = match order.direction {
                    OrderDirection::Buy => order_price >= level_price,
                    OrderDirection::Sell => order_price <= level_price,
                    _ => false,
                };
                if !price_ok {
                    continue;
                }
                let orders = self.get_orders_at_price(opposite_tree, level_price, guard);
                for opposite_order in orders {
                    if matched_qty >= remaining {
                        break;
                    }
                    let opposite_remaining = opposite_order.remaining.load(Ordering::Relaxed);
                    if opposite_remaining <= 0.0 {
                        continue;
                    }
                    let needed_qty = remaining - matched_qty;
                    let match_qty = needed_qty.min(opposite_remaining);
                    if match_qty > 0.0 {
                        match order.direction {
                            OrderDirection::Buy => {
                                LimitOrderHandler.execute_match(
                                    engine,
                                    order,
                                    &opposite_order,
                                    level_price,
                                    match_qty,
                                    symbol,
                                    order_price,
                                    level_price,
                                    false,
                                );
                            }
                            OrderDirection::Sell => {
                                LimitOrderHandler.execute_match(
                                    engine,
                                    &opposite_order,
                                    order,
                                    level_price,
                                    match_qty,
                                    symbol,
                                    order_price,
                                    level_price,
                                    false,
                                );
                            }
                            _ => continue,
                        }
                        matched_qty += match_qty;
                        engine.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                        engine
                            .stats
                            .total_volume
                            .fetch_add(match_qty as u64, Ordering::Relaxed);
                        engine
                            .stats
                            .total_notional
                            .fetch_add((match_qty * level_price as f64) as u64, Ordering::Relaxed);
                    }
                }
            }
            if matched_qty > 0.0 {
                order.filled.fetch_add(matched_qty, Ordering::SeqCst);
                order
                    .remaining
                    .store(remaining - matched_qty, Ordering::SeqCst);
                if matched_qty >= remaining {
                    order.status.store(OrderStatus::Filled, Ordering::SeqCst);
                } else {
                    order.status.store(OrderStatus::Partial, Ordering::SeqCst);
                    if order.order_type == OrderType::IOC {
                        order.status.store(OrderStatus::Cancelled, Ordering::SeqCst);
                    }
                }
            } else {
                order.status.store(OrderStatus::Cancelled, Ordering::SeqCst);
            }
            indices_to_remove.push(index);
        }
        for &index in indices_to_remove.iter().rev() {
            if index < immediate_orders.len() {
                immediate_orders.remove(index);
            }
        }
        Ok(())
    }

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
