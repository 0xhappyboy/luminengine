use super::OrderHandler;
use crate::matcher::limit::LimitOrderHandler;
use crate::matcher::{MatchEngine, OrderMatchContext};
use crate::orderbook::OrderTree;
use crate::orderbook::order::{Order, OrderType};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct DayOrderHandler;

impl OrderHandler for DayOrderHandler {
    fn add_order(
        &self,
        order: Arc<Order>,
        price: u64,
        is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String> {
        // DAY order: set expiration time
        let mut day_map = engine.day_orders.lock();
        let expiry = Instant::now() + Duration::from_secs(24 * 60 * 60);
        day_map.insert(order.id.clone(), expiry);
        // Process as regular limit order
        let ctx = OrderMatchContext::new(order.clone(), price, is_bid);
        engine.pending_limit_orders.lock().push_back(ctx);
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
        self.check_day_order_expiry(engine);
        LimitOrderHandler.process_matching(engine, bids, asks, symbol)
    }
    fn get_order_type(&self) -> OrderType {
        OrderType::DAY
    }
}

impl DayOrderHandler {
    fn check_day_order_expiry(&self, engine: &MatchEngine) {
        let mut day_orders = engine.day_orders.lock();
        let now = Instant::now();
        let mut expired_orders = Vec::new();
        for (order_id, expiry) in day_orders.iter() {
            if now >= *expiry {
                expired_orders.push(order_id.clone());
            }
        }
        for order_id in expired_orders {
            day_orders.remove(&order_id);
            let mut pending_orders = engine.pending_limit_orders.lock();
            pending_orders.retain(|ctx| ctx.order.id != order_id);
        }
    }
}
