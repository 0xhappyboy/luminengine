use std::sync::Arc;

use crate::{
    matchengine::slfe::{Slfe, gtc_manager::GTCOrderManager},
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
    pub async fn start_gtc_manager_task(&self, gtc_manager: Arc<GTCOrderManager>) {
        tokio::spawn(async move { gtc_manager.process_events().await });
    }
    pub async fn handle(slfe: &Slfe, gtc_order: Order) -> UnifiedResult<String> {
        let mut order = gtc_order.clone();
        // No timeout
        order.expiry = None;
        // add an order to gtc manager
        slfe.gtc.add_gtc_order(order.clone()).await?;
        // add limit order
        // Replace the GTC type with the Limit type for processing.
        order.order_type = crate::order::OrderType::Limit;
        slfe.add_order(order);
        Ok("ok".to_string())
    }
}
