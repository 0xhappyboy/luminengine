use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{Duration, interval};

use crate::{
    matchengine::{
        MatchEvent,
        slfe::{
            Slfe,
            processor::{OrderProcessor, limit::LimitOrderProcessor},
        },
    },
    order::{Order, OrderDirection, OrderType},
    types::{UnifiedError, UnifiedResult},
};

const CLEANUP_TASK_INTERVAL: u64 = 30;

#[derive(Debug)]
pub struct DAYOrderProcessor;

impl DAYOrderProcessor {
    pub  fn handle(
        slfe: Arc<Slfe>,
        day_order: Order,
        trading_day_end: Instant,
    ) -> UnifiedResult<String> {
        let mut order = day_order.clone();
        order.order_type = OrderType::Limit;
        order.expiry = Some(trading_day_end);
        LimitOrderProcessor::add(slfe, day_order);
        Ok("".to_string())
    }
}
