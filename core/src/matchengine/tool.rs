pub mod slfe {
    use crate::{matchengine::slfe::sharding::OrderTreeSharding, price::Price};

    /// Calculate the total Quote value of all orders in the entire order tree
    pub fn cal_total_quote_value_for_ordertree<P>(order_tree: &OrderTreeSharding<P>) -> f64
    where
        P: Price + Ord + PartialOrd + PartialEq + Eq + Clone,
    {
        let mut total_quote_value = 0.0;
        for shard in &order_tree.shards {
            let shard_guard = shard.read();
            for (price, orders) in &shard_guard.tree {
                let price_f64 = price.to_f64();
                for order in orders {
                    total_quote_value += order.remaining * price_f64;
                }
            }
        }
        total_quote_value
    }
}

/// order tool
pub mod order {}

/// math tool
pub mod math {
    /// Store f64 in u64 according to the IEEE 754 standard
    pub fn f64_to_atomic(value: f64) -> u64 {
        value.to_bits()
    }
    /// Recover an f64 value from an IEEE 754 standard u64 array.
    pub fn atomic_to_f64(value: u64) -> f64 {
        f64::from_bits(value)
    }
    /// calculate price change (percentage)
    pub fn cal_price_change(current_price: f64, last_price: f64) -> f64 {
        ((current_price - last_price) / last_price).abs()
    }
    /// calculate ewma
    pub fn cal_ewma(price_volatility: f64, price_change: f64) -> f64 {
        (price_volatility * 0.9) + (price_change * 0.1)
    }
    /// calculate mid price
    pub fn cal_mid_price(bid: f64, ask: f64) -> f64 {
        (bid + ask) / 2.0
    }
}
