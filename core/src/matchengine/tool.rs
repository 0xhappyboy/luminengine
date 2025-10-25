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

    /// for performance reasons, floating point prices are converted to u64 for calculation and sorting.
    pub fn f64_to_price_u64(price: f64) -> u64 {
        (price * 10000.0) as u64
    }

    /// restore the price to a floating point number.
    pub fn price_u64_to_f64(price_key: u64) -> f64 {
        price_key as f64 / 10000.0
    }
}

pub mod expiry {
    use std::time::{Duration, Instant};

    use chrono::Timelike;

    /// now to 23:59:59 to day
    pub fn expiry_today_end() -> Instant {
        let now = chrono::Utc::now();
        let end_of_day = now
            .with_hour(23)
            .and_then(|t| t.with_minute(59))
            .and_then(|t| t.with_second(59))
            .unwrap_or(now + chrono::Duration::hours(24));

        let duration_until_end = end_of_day - now;
        Instant::now() + Duration::from_secs(duration_until_end.num_seconds() as u64)
    }

    /// now + 24h
    pub fn expiry_24_hours() -> Instant {
        Instant::now() + Duration::from_secs(24 * 60 * 60)
    }

    /// now + 12h
    pub fn expiry_12_hours() -> Instant {
        Instant::now() + Duration::from_secs(12 * 60 * 60)
    }

    /// now + 1h
    pub fn expiry_1_hour() -> Instant {
        Instant::now() + Duration::from_secs(60 * 60)
    }

    /// now + 30m
    pub fn expiry_30_minutes() -> Instant {
        Instant::now() + Duration::from_secs(30 * 60)
    }

    /// now + 5m
    pub fn expiry_5_minutes() -> Instant {
        Instant::now() + Duration::from_secs(5 * 60)
    }
}
