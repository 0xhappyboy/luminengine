use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{Duration, interval};

use crate::{
    matchengine::{MatchEvent, slfe::Slfe},
    order::{Order, OrderDirection, OrderType},
    types::{UnifiedError, UnifiedResult},
};

const CLEANUP_TASK_INTERVAL: u64 = 30;

#[derive(Debug)]
pub struct DAYOrderProcessor;

impl DAYOrderProcessor {
    pub async fn handle(
        slfe: &Slfe,
        day_order: Order,
        trading_day_end: Instant,
    ) -> UnifiedResult<String> {
        let mut order = day_order.clone();
        order.order_type = OrderType::DAY;
        order.expiry = Some(trading_day_end);
        match slfe.add_order(order.clone()).await {
            Ok(_) => {
                if let Some(location) = slfe.order_location.get(&order.id) {}
                if let Err(e) = slfe.tx.send(MatchEvent::NewLimitOrder) {
                    return Err(UnifiedError::AddLimitOrderError(format!(
                        "Event queue full: {}",
                        e
                    )));
                }
                slfe.update_stats(1, 0, 0.0, Instant::now().elapsed());
                Ok("".to_string())
            }
            Err(e) => Err(e),
        }
    }
}
