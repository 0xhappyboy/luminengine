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
pub struct DAYOrderProcessor {
    expiry_map: Arc<DashMap<String, (Instant, usize, OrderDirection)>>,
}

impl DAYOrderProcessor {
    pub fn new() -> Self {
        Self {
            expiry_map: Arc::new(DashMap::new()),
        }
    }

    pub async fn handle(
        &self,
        slfe: &Slfe,
        day_order: Order,
        trading_day_end: Instant,
    ) -> UnifiedResult<String> {
        let mut order = day_order.clone();
        order.order_type = OrderType::DAY;
        order.expiry = Some(trading_day_end);
        match slfe.add_order(order.clone()).await {
            Ok(_) => {
                if let Some(location) = slfe.order_location.get(&order.id) {
                    self.expiry_map.insert(
                        order.id.clone(),
                        (
                            trading_day_end,
                            location.shard_id,
                            location.direction.clone(),
                        ),
                    );
                }
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

    pub async fn start_expiry_cleanup(&self, slfe: Arc<Slfe>) {
        let mut interval = interval(Duration::from_secs(CLEANUP_TASK_INTERVAL));
        loop {
            interval.tick().await;
            match Self::cleanup_expired_orders(&slfe, &self.expiry_map).await {
                Ok(expired_count) if expired_count > 0 => {}
                Err(e) => {}
                _ => {}
            }
        }
    }

    async fn cleanup_expired_orders(
        slfe: &Slfe,
        expiry_map: &DashMap<String, (Instant, usize, OrderDirection)>,
    ) -> UnifiedResult<usize> {
        let current_time = Instant::now();
        let mut expired_count = 0;
        let mut expired_orders = Vec::new();
        expiry_map.retain(|order_id, (expiry_time, shard_id, direction)| {
            if current_time >= *expiry_time {
                expired_orders.push((order_id.clone(), *shard_id, direction.clone()));
                false
            } else {
                true
            }
        });
        for (order_id, shard_id, direction) in expired_orders {
            if let Err(e) = slfe.tx.send(MatchEvent::CancelOrder(order_id.clone())) {
                continue;
            }
            // Remove from order position mapping table
            slfe.order_location.remove(&order_id);
            expired_count += 1;
        }
        Ok(expired_count)
    }

    pub fn get_expiry_map_stats(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        let mut bid_count = 0;
        let mut ask_count = 0;
        for entry in self.expiry_map.iter() {
            let value = entry.value();
            match value.2 {
                OrderDirection::Buy => bid_count += 1,
                OrderDirection::Sell => ask_count += 1,
                OrderDirection::None => {}
            }
        }
        stats.insert("total".to_string(), self.expiry_map.len());
        stats.insert("bid_orders".to_string(), bid_count);
        stats.insert("ask_orders".to_string(), ask_count);
        let mut shard_distribution = HashMap::new();
        for entry in self.expiry_map.iter() {
            let value = entry.value();
            *shard_distribution.entry(value.1).or_insert(0) += 1;
        }
        for (shard_id, count) in shard_distribution {
            stats.insert(format!("shard_{}", shard_id), count);
        }
        stats
    }
}
