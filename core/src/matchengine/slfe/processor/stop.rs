use std::sync::Arc;

use crate::{
    matchengine::slfe::{Slfe, manager::price_change::StopOrderStatus},
    order::Order,
    types::UnifiedResult,
};

pub struct StopOrderProcessor;

/// stop order processor
impl StopOrderProcessor {
    /// add stop order
    pub fn add_stop_order(
        slfe: Arc<Slfe>,
        original_order: Order,
        stop_price: f64,
        expiry_seconds: Option<u64>,
    ) -> UnifiedResult<String> {
        slfe.price_change_manager
            .add_stop_order(original_order, stop_price, false, None, expiry_seconds)
    }

    /// cancel stop order

    pub fn cancel_stop_order(slfe: Arc<Slfe>, order_id: &str) -> UnifiedResult<bool> {
        slfe.price_change_manager.cancel_stop_order(order_id)
    }

    /// set stop order
    pub fn set_stop_order(
        slfe: Arc<Slfe>,
        order_id: &str,
        new_stop_price: f64,
    ) -> UnifiedResult<bool> {
        slfe.price_change_manager
            .set_stop_order(order_id, new_stop_price, None)
    }

    /// get stop order status
    pub fn get_stop_order_status(slfe: Arc<Slfe>, order_id: &str) -> Option<StopOrderStatus> {
        slfe.price_change_manager.get_stop_order_status(order_id)
    }
}
