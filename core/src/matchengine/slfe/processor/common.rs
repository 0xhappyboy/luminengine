use crate::{
    matchengine::slfe::Slfe,
    order::{Order, OrderDirection},
    price::Price,
};

/// Get the number of transactions available in the shard
pub async fn get_available_quantity_for_order(slfe: &Slfe, order: &Order) -> f64 {
    match order.direction {
        OrderDirection::Buy => {
            let sorted_ask_prices = slfe.asks.get_all_ask_prices_sorted();
            let mut available = 0.0;
            for ask_price in sorted_ask_prices {
                if ask_price.to_f64() > order.price
                    && order.order_type == crate::order::OrderType::Limit
                {
                    break;
                }
                for shard_id in 0..slfe.config.read().shard_count {
                    let shard = slfe.asks.shards[shard_id].read();
                    if let Some(orders) = shard.tree.get(&ask_price) {
                        for counter_order in orders {
                            if counter_order.can_trade() {
                                available += counter_order.remaining;
                            }
                        }
                    }
                }
            }
            available
        }
        OrderDirection::Sell => {
            let sorted_bid_prices = slfe.bids.get_all_bid_prices_sorted();
            let mut available = 0.0;
            for bid_price in sorted_bid_prices {
                if bid_price.to_f64() < order.price
                    && order.order_type == crate::order::OrderType::Limit
                {
                    break;
                }
                for shard_id in 0..slfe.config.read().shard_count {
                    let shard = slfe.bids.shards[shard_id].read();
                    if let Some(orders) = shard.tree.get(&bid_price) {
                        for counter_order in orders {
                            if counter_order.can_trade() {
                                available += counter_order.remaining;
                            }
                        }
                    }
                }
            }
            available
        }
        OrderDirection::None => 0.0,
    }
}
