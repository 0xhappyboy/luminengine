use std::{collections::VecDeque, time::Instant};

use crate::{
    matchengine::{MatchResult, slfe::Slfe},
    order::{Order, OrderDirection, OrderStatus},
    price::{AskPrice, BidPrice, Price},
};

pub struct OrderProcessor;

impl OrderProcessor {
    // =========================================== limit order ===========================================

    /// handle limit order
    pub async fn handle_limit_order(slfe: &Slfe) -> Option<Vec<MatchResult>> {
        let best_bid = slfe.bids.get_best_price();
        let best_ask = slfe.asks.get_best_price();
        if let (Some(bid_price), Some(ask_price)) = (best_bid, best_ask) {
            if bid_price.price >= ask_price.price {
                return Some(
                    Self::limit_order_match_across_shards(slfe, &bid_price, &ask_price).await,
                );
            }
        }
        None
    }
    async fn limit_order_match_across_shards(
        slfe: &Slfe,
        bid_price: &BidPrice,
        ask_price: &AskPrice,
    ) -> Vec<MatchResult> {
        let mut all_results = Vec::new();
        for bid_shard_id in 0..slfe.config.read().shard_count {
            for ask_shard_id in 0..slfe.config.read().shard_count {
                let results = Self::limit_order_match_shard_pair(
                    slfe,
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
    async fn limit_order_match_shard_pair(
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
        let results = Self::limit_order_match_order_queues(&mut bid_orders, &mut ask_orders).await;
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
                bid_shard.tree.remove(bid_price);
            } else {
                bid_shard.tree.insert(bid_price.clone(), bid_orders);
            }
            if ask_orders.is_empty() {
                ask_shard.tree.remove(ask_price);
            } else {
                ask_shard.tree.insert(ask_price.clone(), ask_orders);
            }
        }
        results
    }
    async fn limit_order_match_order_queues(
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
                    bid_orders.remove(bid_index);
                } else {
                    bid_index += 1;
                }
                if ask_order.remaining <= 0.0 {
                    ask_orders.remove(ask_index);
                } else {
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

    // =========================================== market order ===========================================

    /// handle market order
    pub async fn handle_market_order(slfe: &Slfe, market_order: Order) -> Vec<MatchResult> {
        match market_order.direction {
            OrderDirection::Buy => Self::market_order_match_buy_order(slfe, market_order).await,
            OrderDirection::Sell => Self::market_order_match_sell_order(slfe, market_order).await,
            OrderDirection::None => {
                vec![]
            }
        }
    }

    /// match buy orders.
    async fn market_order_match_buy_order(
        slfe: &Slfe,
        mut market_order: Order,
    ) -> Vec<MatchResult> {
        let mut all_results = Vec::new();
        let mut remaining_quantity = market_order.quantity;
        let sorted_ask_prices = slfe.asks.get_all_ask_prices_sorted();
        for ask_price in sorted_ask_prices {
            if remaining_quantity <= 0.0 {
                break;
            }
            let results = Self::market_order_match_price_level(
                slfe,
                &mut market_order,
                &ask_price,
                remaining_quantity,
                OrderDirection::Buy,
            )
            .await;
            for result in &results {
                remaining_quantity -= result.quantity;
            }
            all_results.extend(results);
        }
        if remaining_quantity < market_order.quantity {
            market_order.filled = market_order.quantity - remaining_quantity;
            market_order.remaining = remaining_quantity;
            if remaining_quantity <= 0.0 {
                market_order.status = OrderStatus::Filled;
            } else {
                market_order.status = OrderStatus::Partial;
            }
        }
        all_results
    }

    /// match sell order
    async fn market_order_match_sell_order(
        slfe: &Slfe,
        mut market_order: Order,
    ) -> Vec<MatchResult> {
        let mut all_results = Vec::new();
        let mut remaining_quantity = market_order.quantity;
        let sorted_bid_prices = slfe.bids.get_all_bid_prices_sorted();
        for bid_price in sorted_bid_prices {
            if remaining_quantity <= 0.0 {
                break;
            }
            let results = Self::market_order_match_price_level(
                slfe,
                &mut market_order,
                &bid_price,
                remaining_quantity,
                OrderDirection::Sell,
            )
            .await;
            for result in &results {
                remaining_quantity -= result.quantity;
            }
            all_results.extend(results);
        }
        if remaining_quantity < market_order.quantity {
            market_order.filled = market_order.quantity - remaining_quantity;
            market_order.remaining = remaining_quantity;
            if remaining_quantity <= 0.0 {
                market_order.status = OrderStatus::Filled;
            } else {
                market_order.status = OrderStatus::Partial;
            }
        }
        all_results
    }

    /// Matches orders based on specified price levels.
    async fn market_order_match_price_level(
        slfe: &Slfe,
        market_order: &mut Order,
        price: &impl Price,
        remaining_quantity: f64,
        direction: OrderDirection,
    ) -> Vec<MatchResult> {
        let mut all_results = Vec::new();
        let match_price = price.to_f64();
        for shard_id in 0..slfe.config.read().shard_count {
            if remaining_quantity <= 0.0 {
                break;
            }
            let results = Self::market_order_match_sharding(
                slfe,
                market_order,
                price.to_f64(),
                shard_id,
                remaining_quantity,
                &direction,
            )
            .await;
            let mut matched_qty = 0.0;
            for result in &results {
                matched_qty += result.quantity;
            }
            all_results.extend(results);
        }
        all_results
    }

    /// match sharding
    async fn market_order_match_sharding(
        slfe: &Slfe,
        market_order: &mut Order,
        price: f64,
        shard_id: usize,
        mut remaining_quantity: f64,
        direction: &OrderDirection,
    ) -> Vec<MatchResult> {
        let mut results = Vec::new();
        match direction {
            OrderDirection::Buy => {
                let ask_price = AskPrice::new(price);
                let orders_data = {
                    let shard = slfe.asks.shards[shard_id].read();
                    shard.tree.get(&ask_price).cloned()
                };
                let mut orders = match orders_data {
                    Some(orders) => orders,
                    None => return results,
                };
                let mut index = 0;
                while index < orders.len() && remaining_quantity > 0.0 {
                    let counter_order = &mut orders[index];
                    if counter_order.can_trade() {
                        let match_qty = remaining_quantity.min(counter_order.remaining);
                        if match_qty > 0.0 {
                            results.push(MatchResult {
                                bid_order_id: market_order.id.clone(),
                                ask_order_id: counter_order.id.clone(),
                                price: price,
                                quantity: match_qty,
                                timestamp: Instant::now(),
                            });
                            counter_order.execute_trade(match_qty);
                            remaining_quantity -= match_qty;
                            if counter_order.remaining <= 0.0 {
                                orders.remove(index);
                            } else {
                                index += 1;
                            }
                        } else {
                            index += 1;
                        }
                    } else {
                        index += 1;
                    }
                }
                if !results.is_empty() {
                    let mut ask_shard = slfe.asks.shards[shard_id].write();
                    if orders.is_empty() {
                        ask_shard.tree.remove(&ask_price);
                    } else {
                        ask_shard.tree.insert(ask_price, orders);
                    }
                    let matched_orders = results.len();
                    ask_shard.total_orders = ask_shard.total_orders.saturating_sub(matched_orders);
                }
            }
            OrderDirection::Sell => {
                let bid_price = BidPrice::new(price);
                let orders_data = {
                    let shard = slfe.bids.shards[shard_id].read();
                    shard.tree.get(&bid_price).cloned()
                };
                let mut orders = match orders_data {
                    Some(orders) => orders,
                    None => return results,
                };
                let mut index = 0;
                while index < orders.len() && remaining_quantity > 0.0 {
                    let counter_order = &mut orders[index];
                    if counter_order.can_trade() {
                        let match_qty = remaining_quantity.min(counter_order.remaining);
                        if match_qty > 0.0 {
                            results.push(MatchResult {
                                bid_order_id: counter_order.id.clone(),
                                ask_order_id: market_order.id.clone(),
                                price: price,
                                quantity: match_qty,
                                timestamp: Instant::now(),
                            });
                            counter_order.execute_trade(match_qty);
                            remaining_quantity -= match_qty;
                            if counter_order.remaining <= 0.0 {
                                orders.remove(index);
                            } else {
                                index += 1;
                            }
                        } else {
                            index += 1;
                        }
                    } else {
                        index += 1;
                    }
                }
                if !results.is_empty() {
                    let mut bid_shard = slfe.bids.shards[shard_id].write();
                    if orders.is_empty() {
                        bid_shard.tree.remove(&bid_price);
                    } else {
                        bid_shard.tree.insert(bid_price, orders);
                    }
                    let matched_orders = results.len();
                    bid_shard.total_orders = bid_shard.total_orders.saturating_sub(matched_orders);
                }
            }
            OrderDirection::None => {
                // .....
            }
        }
        results
    }
}
