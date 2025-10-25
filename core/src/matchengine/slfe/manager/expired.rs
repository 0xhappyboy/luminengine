use std::thread;
use std::time::Instant;
use std::{sync::Arc, thread::sleep};
use tokio::time::Duration;

use crate::{
    matchengine::slfe::{Slfe, sharding::OrderTreeSharding},
    types::UnifiedResult,
};

const CLEANUP_TASK_INTERVAL: u64 = 30;

#[derive(Debug)]
pub struct ExpiredOrderManager;

impl ExpiredOrderManager {
    pub fn new() -> Self {
        Self {}
    }
    pub fn start_expiry_order_manager(&self, slfe: Arc<Slfe>) {
        loop {
            match Self::cleanup_expired_orders(slfe.clone()) {
                Ok(expired_count) if expired_count > 0 => {}
                Err(e) => {}
                _ => {}
            }
            sleep(Duration::from_secs(CLEANUP_TASK_INTERVAL));
        }
    }

    pub fn cleanup_expired_orders(slfe: Arc<Slfe>) -> UnifiedResult<usize> {
        let bids_arc = slfe.bids.clone();
        let asks_arc = slfe.asks.clone();
        let bid_handle = thread::spawn(move || Self::cleanup_single_direction(&bids_arc));
        let ask_handle = thread::spawn(move || Self::cleanup_single_direction(&asks_arc));
        let bid_removed = bid_handle.join().unwrap_or(0);
        let ask_removed = ask_handle.join().unwrap_or(0);
        Ok(bid_removed + ask_removed)
    }

    fn cleanup_single_direction<P>(order_tree: &OrderTreeSharding<P>) -> usize
    where
        P: crate::price::Price + Ord + PartialOrd + PartialEq + Eq + Clone,
    {
        let mut total_removed = 0;
        let now = Instant::now();
        for shard in &order_tree.shards {
            let mut shard_guard = shard.write();
            let mut prices_to_remove = Vec::new();
            let mut shard_removed = 0;
            for (price, orders) in &mut shard_guard.tree {
                let mut i = 0;
                while i < orders.len() {
                    if let Some(expiry) = orders[i].expiry {
                        if now > expiry {
                            orders.remove(i);
                            shard_removed += 1;
                            continue;
                        }
                    }
                    i += 1;
                }
                if orders.is_empty() {
                    prices_to_remove.push(price.clone());
                }
            }
            shard_guard.total_orders -= shard_removed;
            total_removed += shard_removed;
            for price in prices_to_remove {
                shard_guard.tree.remove(&price);
            }
        }
        total_removed
    }
}
