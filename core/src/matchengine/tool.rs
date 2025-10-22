pub mod slfe {
    use crate::{matchengine::slfe::ShardedOrderTree, price::Price};

    /// Calculate the total Quote value of all orders in the entire order tree
    pub fn cal_total_quote_value_for_ordertree<P>(order_tree: &ShardedOrderTree<P>) -> f64
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
