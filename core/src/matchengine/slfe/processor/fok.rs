use crate::{
    matchengine::{
        MatchResult,
        slfe::{Slfe, processor::market::MarketOrderProcessor},
    },
    order::Order,
};

pub struct FOKOrderProcessor;
impl FOKOrderProcessor {
    pub async fn handle(slfe: &Slfe, order: Order) -> Vec<MatchResult> {
        // Determine whether the fok order can be fully executed.
        let available_quantity = slfe.get_available_quantity_for_order(&order).await;
        if available_quantity >= order.quantity {
            MarketOrderProcessor::handle_market_order(slfe, order).await
        } else {
            Vec::new()
        }
    }
}
