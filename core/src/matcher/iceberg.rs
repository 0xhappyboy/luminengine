use super::OrderHandler;
use crate::{
    matcher::limit::LimitOrderHandler,
    matcher::{MatchEngine, OrderMatchContext},
    orderbook::OrderTree,
    orderbook::order::{Order, OrderType},
};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub struct IcebergOrderHandler;

impl OrderHandler for IcebergOrderHandler {
    fn add_order(
        &self,
        order: Arc<Order>,
        price: u64,
        is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String> {
        // Iceberg order: special handling
        let mut iceberg_map = engine.iceberg_orders.lock();
        let mut ctx = OrderMatchContext::new(order.clone(), price, is_bid);
        let total_qty = order.quantity.load(Ordering::Relaxed);
        ctx.iceberg_remaining = total_qty - ctx.iceberg_display_size;
        order
            .quantity
            .store(ctx.iceberg_display_size, Ordering::SeqCst);
        iceberg_map.insert(order.id.clone(), ctx);
        // Also process as regular limit order
        let ctx_for_queue = OrderMatchContext::new(order.clone(), price, is_bid);
        engine.pending_limit_orders.lock().push_back(ctx_for_queue);
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
        self.process_iceberg_slices(engine);
        self.process_iceberg_orders(engine, bids, asks, symbol)
    }

    fn get_order_type(&self) -> OrderType {
        OrderType::Iceberg
    }
}

impl IcebergOrderHandler {
    fn process_iceberg_orders(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        LimitOrderHandler.process_matching(engine, bids, asks, symbol)?;
        self.update_iceberg_display(engine);
        Ok(())
    }

    fn process_iceberg_slices(&self, engine: &MatchEngine) {
        let mut iceberg_orders = engine.iceberg_orders.lock();
        let mut processed_orders = Vec::new();
        let now = Instant::now();
        for (order_id, ctx) in iceberg_orders.iter_mut() {
            let should_show_slice = match ctx.next_iceberg_slice_time {
                Some(next_time) => now >= next_time,
                None => true,
            };
            if should_show_slice && ctx.iceberg_remaining > 0.0 {
                let slice_qty = ctx.iceberg_display_size.min(ctx.iceberg_remaining);
                ctx.order.quantity.store(slice_qty, Ordering::SeqCst);
                ctx.iceberg_remaining -= slice_qty;
                ctx.next_iceberg_slice_time = Some(now + Duration::from_millis(100));
                engine.stats.iceberg_slices.fetch_add(1, Ordering::Relaxed);
            }
            if ctx.iceberg_remaining <= 0.0 && ctx.order.remaining.load(Ordering::Relaxed) <= 0.0 {
                processed_orders.push(order_id.clone());
            }
        }
        for order_id in processed_orders {
            iceberg_orders.remove(&order_id);
        }
    }

    fn update_iceberg_display(&self, engine: &MatchEngine) {
        let mut iceberg_orders = engine.iceberg_orders.lock();
        for (_order_id, ctx) in iceberg_orders.iter_mut() {
            let order = &ctx.order;
            let current_qty = order.quantity.load(Ordering::Relaxed);
            let remaining = order.remaining.load(Ordering::Relaxed);
            if remaining <= 0.0 && ctx.iceberg_remaining > 0.0 {
                let next_slice = ctx.iceberg_display_size.min(ctx.iceberg_remaining);
                order.quantity.store(next_slice, Ordering::SeqCst);
                order.remaining.store(next_slice, Ordering::SeqCst);
                ctx.iceberg_remaining -= next_slice;
                ctx.next_iceberg_slice_time = Some(Instant::now() + Duration::from_millis(100));
                let price = ctx.price;
                let is_bid = ctx.is_bid;
                let new_ctx = OrderMatchContext::new(order.clone(), price, is_bid);
                engine.pending_limit_orders.lock().push_back(new_ctx);
            }
        }
    }

    fn update_iceberg_order(&self, engine: &MatchEngine, order: &Arc<Order>, matched_qty: f64) {
        let mut iceberg_orders = engine.iceberg_orders.lock();
        if let Some(ctx) = iceberg_orders.get_mut(&order.id) {
            let current_qty = order.quantity.load(Ordering::Relaxed);
            let new_qty = current_qty - matched_qty;
            if new_qty <= 0.0 && ctx.iceberg_remaining > 0.0 {
                let next_slice = ctx.iceberg_display_size.min(ctx.iceberg_remaining);
                order.quantity.store(next_slice, Ordering::SeqCst);
                ctx.iceberg_remaining -= next_slice;
                ctx.next_iceberg_slice_time = Some(Instant::now() + Duration::from_millis(100));
            } else {
                order.quantity.store(new_qty, Ordering::SeqCst);
            }
        }
    }
}
