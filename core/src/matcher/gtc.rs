use super::OrderHandler;
use crate::{
    matcher::limit::LimitOrderHandler,
    matcher::{MatchEngine, OrderMatchContext},
    orderbook::OrderTree,
    orderbook::order::{Order, OrderType},
};
use std::sync::Arc;

pub struct GTCOrderHandler;

impl OrderHandler for GTCOrderHandler {
    fn add_order(
        &self,
        order: Arc<Order>,
        price: u64,
        is_bid: bool,
        engine: &MatchEngine,
    ) -> Result<(), String> {
        // GTC order: no expiration time
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
        LimitOrderHandler.process_matching(engine, bids, asks, symbol)
    }

    fn get_order_type(&self) -> OrderType {
        OrderType::GTC
    }
}
