use super::OrderHandler;
use crate::{
    matcher::MatchEngine,
    orderbook::order::{Order, OrderType, OrderDirection},
    orderbook::OrderTree,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct StopOrderHandler;

impl OrderHandler for StopOrderHandler {
    fn add_order(
        &self,
        order: Arc<Order>,
        _price: u64,
        _is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String> {
        // Stop order: wait for trigger
        engine.stop_order_queue.lock().push_back(order);
        Ok(())
    }

    fn process_matching(
        &self,
        engine: &MatchEngine,
        bids: &OrderTree,
        asks: &OrderTree,
        symbol: &Arc<str>,
    ) -> Result<(), String> {
        self.check_stop_orders(engine, bids, asks, symbol)
    }

    fn get_order_type(&self) -> OrderType {
        OrderType::Stop
    }
}

impl StopOrderHandler {
    fn check_stop_orders(
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
        let mut stop_orders = engine.stop_order_queue.lock();
        let mut triggered_orders = Vec::new();
        for order in stop_orders.iter() {
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
                triggered_orders.push(order.clone());
                engine.stats.stop_triggers.fetch_add(1, Ordering::Relaxed);
            }
        }
        for order in triggered_orders {
            stop_orders.retain(|o| o.id != order.id);
            engine.market_order_queue.lock().push_back(order);
        }
        Ok(())
    }
}