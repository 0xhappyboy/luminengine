use std::sync::Arc;

use crate::{
    matchengine::slfe::{manager::price_change::StopOrderStatus, Slfe  },
    order::Order,
    types::UnifiedResult,
};

pub struct StopLimitOrderProcessor;

/// stop limit order processor
impl StopLimitOrderProcessor {
    /// add stop limit order
    pub fn add_stop_limit_order(
        slfe: Arc<Slfe>,
        original_order: Order,
        stop_price: f64,
        limit_price: f64,
        expiry_seconds: Option<u64>,
    ) -> UnifiedResult<String> {
        slfe.price_change_manager.add_stop_order(
            original_order,
            stop_price,
            true,
            Some(limit_price),
            expiry_seconds,
        )
    }

    /// cancel stop limit order
    pub fn cancel_stop_limit_order(slfe: Arc<Slfe>, order_id: &str) -> UnifiedResult<bool> {
        slfe.price_change_manager.cancel_stop_order(order_id)
    }

    /// set stop limit order
    pub fn set_stop_limit_order(
        slfe: Arc<Slfe>,
        order_id: &str,
        new_stop_price: f64,
        new_limit_price: f64,
    ) -> UnifiedResult<bool> {
        slfe.price_change_manager
            .set_stop_order(order_id, new_stop_price, Some(new_limit_price))
    }

    /// get stop limit order status
    pub fn get_stop_limit_order_status(slfe: Arc<Slfe>, order_id: &str) -> Option<StopOrderStatus> {
        slfe.price_change_manager.get_stop_order_status(order_id)
    }
}
