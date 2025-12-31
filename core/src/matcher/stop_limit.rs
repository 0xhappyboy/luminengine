use super::OrderHandler;
use crate::{
    matcher::{MatchEngine, OrderMatchContext},
    orderbook::OrderTree,
    orderbook::order::{Order, OrderDirection, OrderType},
};
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct StopLimitOrderHandler;

impl OrderHandler for StopLimitOrderHandler {
    fn add_order(
        &self,
        order: Arc<Order>,
        _price: u64,
        _is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String> {
        // Stop-limit order: wait for trigger
        engine.stop_limit_order_queue.lock().push_back(order);
        Ok(())
    }

    fn process_matching(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        self.check_stop_limit_orders(engine, bids, asks, symbol)
    }

    fn get_order_type(&self) -> OrderType {
        OrderType::StopLimit
    }
}

impl StopLimitOrderHandler {
    fn check_stop_limit_orders(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        _symbol: &Arc<str>,
    ) -> Result<(), String> {
        let bid_levels = bids.get_price_levels(1);
        let ask_levels = asks.get_price_levels(1);
        if bid_levels.is_empty() || ask_levels.is_empty() {
            return Ok(());
        }
        let best_bid = bid_levels[0].price as u64;
        let best_ask = ask_levels[0].price as u64;
        let mut stop_limit_orders = engine.stop_limit_order_queue.lock();
        let mut triggered_limit_orders = Vec::new();
        for order in stop_limit_orders.iter() {
            let stop_price = order.price.load(Ordering::Relaxed) as u64;
            let market_price = match order.direction {
                OrderDirection::Buy => best_ask,
                OrderDirection::Sell => best_bid,
                _ => continue,
            };
            let triggered = match order.direction {
                OrderDirection::Buy => market_price >= stop_price,
                OrderDirection::Sell => market_price <= stop_price,
                _ => false,
            };
            if triggered {
                triggered_limit_orders.push(order.clone());
                engine.stats.stop_triggers.fetch_add(1, Ordering::Relaxed);
            }
        }
        for order in triggered_limit_orders {
            stop_limit_orders.retain(|o| o.id != order.id);
            let ctx = OrderMatchContext::new(
                order.clone(),
                order.price.load(Ordering::Relaxed) as u64,
                order.direction == OrderDirection::Buy,
            );
            engine.pending_limit_orders.lock().push_back(ctx);
        }
        Ok(())
    }
}
