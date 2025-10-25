use std::sync::Arc;

use crate::{
    matchengine::{
        MatchResult,
        slfe::{Slfe, processor::market::MarketOrderProcessor},
    },
    order::Order,
};

pub struct FOKOrderProcessor;
impl FOKOrderProcessor {
    pub fn handle(slfe: Arc<Slfe>, order: Order) -> Vec<MatchResult> {
        // Determine whether the fok order can be fully executed.
        let available_quantity = slfe.get_available_quantity_for_order(&order);
        if available_quantity >= order.quantity {
            MarketOrderProcessor::handle(slfe.clone(), order)
        } else {
            Vec::new()
        }
    }
}
