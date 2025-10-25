use std::{sync::Arc, time::Instant};

use crate::{
    matchengine::{MatchResult, slfe::Slfe},
    order::{Order, OrderDirection, OrderStatus},
    price::{AskPrice, BidPrice, Price},
};

pub struct MarketOrderProcessor;

impl MarketOrderProcessor {
    /// handle market order
    pub fn handle(slfe: Arc<Slfe>, market_order: Order) -> Vec<MatchResult> {
        match market_order.direction {
            OrderDirection::Buy => Self::market_order_match_buy_order(slfe, market_order),
            OrderDirection::Sell => Self::market_order_match_sell_order(slfe, market_order),
            OrderDirection::None => {
                vec![]
            }
        }
    }

    /// match buy orders.
    fn market_order_match_buy_order(slfe: Arc<Slfe>, mut market_order: Order) -> Vec<MatchResult> {
        let mut all_results = Vec::new();
        let mut remaining_quantity = market_order.quantity;
        let sorted_ask_prices = slfe.asks.get_all_ask_prices_sorted();
        for ask_price in sorted_ask_prices {
            if remaining_quantity <= 0.0 {
                break;
            }
            let results = Self::market_order_match_price_level(
                slfe.clone(),
                &mut market_order,
                &ask_price,
                remaining_quantity,
                OrderDirection::Buy,
            );
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
    fn market_order_match_sell_order(slfe: Arc<Slfe>, mut market_order: Order) -> Vec<MatchResult> {
        let mut all_results = Vec::new();
        let mut remaining_quantity = market_order.quantity;
        let sorted_bid_prices = slfe.bids.get_all_bid_prices_sorted();
        for bid_price in sorted_bid_prices {
            if remaining_quantity <= 0.0 {
                break;
            }
            let results = Self::market_order_match_price_level(
                slfe.clone(),
                &mut market_order,
                &bid_price,
                remaining_quantity,
                OrderDirection::Sell,
            );
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
    fn market_order_match_price_level(
        slfe: Arc<Slfe>,
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
                slfe.clone(),
                market_order,
                price.to_f64(),
                shard_id,
                remaining_quantity,
                &direction,
            );
            let mut matched_qty = 0.0;
            for result in &results {
                matched_qty += result.quantity;
            }
            all_results.extend(results);
        }
        all_results
    }

    /// match sharding
    fn market_order_match_sharding(
        slfe: Arc<Slfe>,
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
