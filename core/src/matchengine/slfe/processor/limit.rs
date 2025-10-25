use std::{collections::VecDeque, sync::Arc, time::Instant};

use crate::{
    matchengine::{MatchEvent, MatchResult, slfe::Slfe},
    order::{Order, OrderDirection},
    price::{AskPrice, BidPrice},
    types::{UnifiedError, UnifiedResult},
};

pub struct LimitOrderProcessor;

impl LimitOrderProcessor {
    /// Added unified entry for limit orders to the order book.
    pub async fn add(slfe: Arc<Slfe>, order: Order) -> UnifiedResult<String> {
        let start_time = Instant::now();
        let location = match order.direction {
            OrderDirection::Buy => slfe.bids.add_order(order.clone()),
            OrderDirection::Sell => slfe.asks.add_order(order.clone()),
            OrderDirection::None => {
                return Err(UnifiedError::AddLimitOrderError(format!(
                    "Order Direction ERROR"
                )));
            }
        };
        // record the location of the latest order in the shard
        slfe.order_location.insert(order.id.clone(), location);
        // send event
        if let Err(e) = slfe.event_manager.send_event(MatchEvent::NewLimitOrder) {
            return Err(UnifiedError::AddLimitOrderError(format!(
                "Event queue full: {:?}",
                e
            )));
        }
        // update status
        slfe.status_manager
            .update(slfe.as_ref(), 1, 0, 0.0, start_time.elapsed());
        return Ok("".to_string());
    }

    /// handle limit order
    /// Used in event managers.
    pub async fn handle(slfe: &Slfe) -> Option<Vec<MatchResult>> {
        let best_bid = slfe.bids.get_best_price();
        let best_ask = slfe.asks.get_best_price();
        if let (Some(bid_price), Some(ask_price)) = (best_bid, best_ask) {
            if bid_price.price >= ask_price.price {
                return Some(Self::match_across_shards(slfe, &bid_price, &ask_price).await);
            }
        }
        None
    }
    async fn match_across_shards(
        slfe: &Slfe,
        bid_price: &BidPrice,
        ask_price: &AskPrice,
    ) -> Vec<MatchResult> {
        let mut all_results = Vec::new();
        for bid_shard_id in 0..slfe.config.read().shard_count {
            for ask_shard_id in 0..slfe.config.read().shard_count {
                let results = Self::match_shard_pair(
                    slfe.clone(),
                    bid_shard_id,
                    ask_shard_id,
                    bid_price,
                    ask_price,
                )
                .await;
                all_results.extend(results);
            }
        }
        all_results
    }
    async fn match_shard_pair(
        slfe: &Slfe,
        bid_shard_id: usize,
        ask_shard_id: usize,
        bid_price: &BidPrice,
        ask_price: &AskPrice,
    ) -> Vec<MatchResult> {
        let (bid_orders_data, ask_orders_data) = {
            let bid_shard = slfe.bids.shards[bid_shard_id].read();
            let ask_shard = slfe.asks.shards[ask_shard_id].read();
            let bid_orders = bid_shard.tree.get(bid_price).cloned();
            let ask_orders = ask_shard.tree.get(ask_price).cloned();
            (bid_orders, ask_orders)
        };
        let (mut bid_orders, mut ask_orders) = match (bid_orders_data, ask_orders_data) {
            (Some(bid), Some(ask)) => (bid, ask),
            _ => return Vec::new(),
        };
        let original_bid_count = bid_orders.len();
        let original_ask_count = ask_orders.len();
        let results =
            Self::match_order_queues(slfe.clone(), &mut bid_orders, &mut ask_orders).await;
        if results.is_empty() {
            return results;
        }
        {
            let mut bid_shard = slfe.bids.shards[bid_shard_id].write();
            let mut ask_shard = slfe.asks.shards[ask_shard_id].write();
            let matched_bid_orders = original_bid_count - bid_orders.len();
            let matched_ask_orders = original_ask_count - ask_orders.len();
            bid_shard.total_orders -= matched_bid_orders;
            ask_shard.total_orders -= matched_ask_orders;
            if bid_orders.is_empty() {
                // When there are no orders at this price level, delete the entire order queue.
                bid_shard.tree.remove(bid_price);
            } else {
                bid_shard.tree.insert(bid_price.clone(), bid_orders);
            }
            if ask_orders.is_empty() {
                // When there are no orders at this price level, delete the entire order queue.
                ask_shard.tree.remove(ask_price);
            } else {
                ask_shard.tree.insert(ask_price.clone(), ask_orders);
            }
        }
        results
    }
    async fn match_order_queues(
        slfe: &Slfe,
        bid_orders: &mut VecDeque<Order>,
        ask_orders: &mut VecDeque<Order>,
    ) -> Vec<MatchResult> {
        let mut results = Vec::new();
        let match_price = bid_orders
            .front()
            .unwrap()
            .price
            .min(ask_orders.front().unwrap().price);
        let mut bid_index = 0;
        let mut ask_index = 0;
        while bid_index < bid_orders.len() && ask_index < ask_orders.len() {
            let bid_order = &mut bid_orders[bid_index];
            let ask_order = &mut ask_orders[ask_index];
            if bid_order.can_trade() && ask_order.can_trade() {
                let match_qty = bid_order.remaining.min(ask_order.remaining);
                if match_qty > 0.0 {
                    results.push(MatchResult {
                        bid_order_id: bid_order.id.clone(),
                        ask_order_id: ask_order.id.clone(),
                        price: match_price,
                        quantity: match_qty,
                        timestamp: Instant::now(),
                    });
                    bid_order.execute_trade(match_qty);
                    ask_order.execute_trade(match_qty);
                }
                if bid_order.remaining <= 0.0 {
                    // notify iceberg manager
                    bid_order.notify_iceberg_manager(&slfe.iceberg_manager);
                    bid_order.notify_gtc_manager(&slfe.gtc_manager);
                    // complete deal
                    bid_orders.remove(bid_index);
                } else {
                    // Partial deal
                    bid_order.notify_gtc_manager(&slfe.gtc_manager);
                    bid_index += 1;
                }
                if ask_order.remaining <= 0.0 {
                    // notify iceberg manager
                    ask_order.notify_iceberg_manager(&slfe.iceberg_manager);
                    ask_order.notify_gtc_manager(&slfe.gtc_manager);
                    // complete deal
                    ask_orders.remove(ask_index);
                } else {
                    // Partial deal
                    ask_order.notify_gtc_manager(&slfe.gtc_manager);
                    ask_index += 1;
                }
            } else {
                if !bid_order.can_trade() {
                    bid_index += 1;
                }
                if !ask_order.can_trade() {
                    ask_index += 1;
                }
            }
        }
        results
    }
}
