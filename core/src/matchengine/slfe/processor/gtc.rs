use std::sync::Arc;

use crate::{
    matchengine::slfe::{Slfe, manager::gtc::GTCOrderManager, processor::OrderProcessor},
    order::Order,
    types::UnifiedResult,
};

pub struct GTCOrderProcessor {
    gtc_manager: Arc<GTCOrderManager>,
}

impl GTCOrderProcessor {
    pub fn new(gtc_manager: Arc<GTCOrderManager>) -> Self {
        Self { gtc_manager }
    }
    pub fn start_gtc_manager_task(&self, gtc_manager: Arc<GTCOrderManager>) {
        tokio::spawn(async move { gtc_manager.process_events() });
    }
    pub fn handle(slfe: Arc<Slfe>, gtc_order: Order) -> UnifiedResult<String> {
        let mut order = gtc_order.clone();
        // No timeout
        order.expiry = None;
        // add an order to gtc manager
        slfe.gtc_manager.add_gtc_order(order.clone())?;
        // add limit order
        // Replace the GTC type with the Limit type for processing.
        order.order_type = crate::order::OrderType::Limit;
        OrderProcessor::handle_new_order(slfe.clone(), order);
        Ok("ok".to_string())
    }
}
