use std::{
    collections::{BTreeMap, VecDeque},
    sync::atomic::{AtomicUsize, Ordering},
};

use parking_lot::RwLock;

use crate::{
    order::{Order, OrderDirection, OrderTree},
    price::{AskPrice, BidPrice, Price},
};

#[derive(Debug, Clone)]
pub struct OrderLocation {
    pub price_key: u64,
    pub direction: OrderDirection,
    pub shard_id: usize,
    pub order_id: String,
}

#[derive(Debug)]
pub struct OrderTreeSharding<P>
where
    P: Price + Ord + PartialOrd + PartialEq + Eq + Clone,
{
    pub shards: Vec<RwLock<OrderTree<P>>>,
    pub shard_count: AtomicUsize,
}

impl<P> OrderTreeSharding<P>
where
    P: Price + Ord + PartialOrd + PartialEq + Eq + Clone,
{
    pub fn new(shard_count: usize) -> Self {
        let mut shards = Vec::with_capacity(shard_count);
        for _ in 0..shard_count {
            shards.push(RwLock::new(OrderTree {
                tree: BTreeMap::new(),
                total_orders: 0,
            }));
        }
        Self {
            shards,
            shard_count: AtomicUsize::new(shard_count),
        }
    }
    fn get_shard_index(&self, order_id: &str) -> usize {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        order_id.hash(&mut hasher);
        (hasher.finish() as usize) % self.shard_count.load(Ordering::SeqCst)
    }
    pub fn add_order(&self, order: Order) -> OrderLocation {
        let shard_index = self.get_shard_index(&order.id);
        let price_key = (order.price * 10000.0) as u64;

        {
            let mut shard = self.shards[shard_index].write();
            let price = P::new(order.price);
            shard
                .tree
                .entry(price)
                .or_insert_with(VecDeque::new)
                .push_back(order.clone());
            shard.total_orders += 1;
        }
        OrderLocation {
            price_key,
            direction: order.direction,
            shard_id: shard_index,
            order_id: order.id,
        }
    }
    pub fn get_best_price(&self) -> Option<P> {
        let mut best_price: Option<P> = None;
        for shard in &self.shards {
            let shard_guard = shard.read();
            let shard_best = if std::any::TypeId::of::<P>() == std::any::TypeId::of::<BidPrice>() {
                shard_guard.tree.keys().last().cloned()
            } else {
                shard_guard.tree.keys().next().cloned()
            };
            if let Some(price) = shard_best {
                match &best_price {
                    None => best_price = Some(price),
                    Some(current_best) => {
                        if (std::any::TypeId::of::<P>() == std::any::TypeId::of::<BidPrice>()
                            && price > *current_best)
                            || (std::any::TypeId::of::<P>() != std::any::TypeId::of::<BidPrice>()
                                && price < *current_best)
                        {
                            best_price = Some(price);
                        }
                    }
                }
            }
        }
        best_price
    }
    pub fn get_total_order_count(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.read().total_orders)
            .sum()
    }
    pub fn remove_order(&self, order_id: &str, shard_id: usize) -> bool {
        let mut shard = self.shards[shard_id].write();
        let operation_result = shard
            .tree
            .iter()
            .find_map(|(price, orders)| {
                orders
                    .iter()
                    .position(|order| order.id == order_id)
                    .map(|pos| (price.clone(), pos, orders.len()))
            })
            .and_then(|(price, index, original_len)| {
                shard.tree.get_mut(&price).map(|orders| {
                    orders.remove(index);
                    (price, original_len == 1)
                })
            });
        if let Some((price, is_empty)) = operation_result {
            shard.total_orders -= 1;
            if is_empty {
                shard.tree.remove(&price);
            }
            true
        } else {
            false
        }
    }
    /// Get the total order quantity at a specified price.
    pub fn get_order_count_by_price(&self, price: f64) -> usize {
        let price_key = P::new(price);
        let mut total_count = 0;

        for shard in &self.shards {
            let shard_guard = shard.read();
            if let Some(orders) = shard_guard.tree.get(&price_key) {
                total_count += orders.len();
            }
        }
        total_count
    }
    /// Get the total base quantity at the specified price.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │    INPUT PRICE   │   ORDER STATE    │  BASE TOKEN      │  RETURN VALUE    │
    /// │   (QUOTE)        │   AT PRICE LEVEL │  CALCULATION     │  (BASE TOTAL)    │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 50000.0 │ Shard0: [0.5 BTC,│ 0.5 + 1.2 + 0.3  │      2.0 BTC     │
    /// │     (USDT)       │ 1.2 BTC]         │                  │                  │
    /// │                  │ Shard1: [0.3 BTC]│                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 51000.0 │ All shards: []   │ 0.0              │      0.0 BTC     │
    /// │     (USDT)       │ (No orders)      │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 2.5     │ Shard0: [1000 ETH│ 1000 + 500       │     1500 ETH     │
    /// │     (USDT)       │ Shard1: [500 ETH]│                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 1.0     │ Shard2: [50000   │ 50000            │    50000 ADA     │
    /// │     (USDT)       │ ADA]             │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    pub fn get_total_base_unit_by_price(&self, price: f64) -> f64 {
        let price_key = P::new(price);
        let mut total_quantity = 0.0;
        for shard in &self.shards {
            let shard_guard = shard.read();
            if let Some(orders) = shard_guard.tree.get(&price_key) {
                for order in orders {
                    total_quantity += order.remaining;
                }
            }
        }
        total_quantity
    }

    /// Get the total value of the quote for the specified price.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │    INPUT PRICE   │   ORDER STATE    │  QUOTE TOKEN     │  RETURN VALUE    │
    /// │   (QUOTE)        │   AT PRICE LEVEL │  CALCULATION     │  (QUOTE TOTAL)   │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 50000.0 │ Shard0: [0.5 BTC,│ (0.5 × 50000) +  │   100000.0 USD   │
    /// │     (USDT)       │ 1.2 BTC]         │ (1.2 × 50000) +  │                  │
    /// │                  │ Shard1: [0.3 BTC]│ (0.3 × 50000)    │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 51000.0 │ All shards: []   │ 0.0              │      0.0 USD     │
    /// │     (USDT)       │ (No orders)      │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 2.5     │ Shard0: [1000 ETH│ (1000 × 2.5) +   │     3750.0 USD   │
    /// │     (USDT)       │ Shard1: [500 ETH]│ (500 × 2.5)      │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  price = 1.0     │ Shard2: [50000   │ 50000 × 1.0      │   50000.0 USD    │
    /// │     (USDT)       │ ADA]             │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    pub fn get_total_quote_value_by_price(&self, price: f64) -> f64 {
        let price_key = P::new(price);
        let mut total_quote_value = 0.0;
        for shard in &self.shards {
            let shard_guard = shard.read();
            if let Some(orders) = shard_guard.tree.get(&price_key) {
                for order in orders {
                    // quote_value = base_quantity * price
                    total_quote_value += order.remaining * price;
                }
            }
        }
        total_quote_value
    }

    /// Get a map of all price tiers relative to order quantities.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │   PRICE LEVEL    │   ORDER STATE    │  ORDER COUNT     │  RETURN VALUE    │
    /// │    (Quote)       │   AT PRICE LEVEL │  CALCULATION     │  (Order Total)   │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50000.0         │ [OrderA, OrderB, │ 2 + 1 = 3        │        3         │
    /// │  (USDT/BTC)      │ OrderC]+[OrderD] │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50100.0         │ [OrderE]         │ 1                │        1         │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50200.0         │ [OrderF, OrderG] │ 2                │        2         │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  49900.0         │ [] (No orders)   │ 0                │        0         │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    /// # Returns
    /// * map { k: price level, v: order count }
    pub fn get_price_level_total_order(&self) -> BTreeMap<u64, usize> {
        let mut distribution = BTreeMap::new();
        for shard in &self.shards {
            let shard_guard = shard.read();
            for (price, orders) in &shard_guard.tree {
                let price_key = (price.to_f64() * 10000.0) as u64;
                *distribution.entry(price_key).or_insert(0) += orders.len();
            }
        }
        distribution
    }
    /// Gets a map of the sum of base units for all price levels.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │   PRICE LEVEL    │   ORDER STATE    │  BASE QUANTITY   │  RETURN VALUE    │
    /// │    (Quote)       │   AT PRICE LEVEL │  CALCULATION     │  (Base Total)    │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50000.0         │ [0.5 BTC,1.2 BTC │ 0.5 + 1.2 + 0.3  │      2.0 BTC     │
    /// │  (USDT/BTC)      │ ] + [0.3 BTC]    │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  2500.0          │ [1000 ETH,       │ 1000 + 500       │     1500 ETH     │
    /// │  (USDT/ETH)      │ 500 ETH]         │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  1.0             │ [50000 ADA]      │ 50000            │    50000 ADA     │
    /// │  (USDT/ADA)      │                  │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  51000.0         │ [] (No orders)   │ 0.0              │      0.0 BTC     │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    /// # Returns
    /// * map { k: price level, v: base unit total count }
    pub fn get_price_level_total_base_unit(&self) -> BTreeMap<u64, f64> {
        let mut distribution = BTreeMap::new();
        for shard in &self.shards {
            let shard_guard = shard.read();
            for (price, orders) in &shard_guard.tree {
                let price_key = (price.to_f64() * 10000.0) as u64;
                let total_quantity: f64 = orders.iter().map(|order| order.remaining).sum();
                *distribution.entry(price_key).or_insert(0.0) += total_quantity;
            }
        }
        distribution
    }

    /// Get the total value of quotes for all price levels.
    /// # Example
    /// ┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
    /// │   PRICE LEVEL    │   ORDER STATE    │  QUOTE VALUE     │  RETURN VALUE    │
    /// │    (Quote)       │   AT PRICE LEVEL │  CALCULATION     │  (Quote Total)   │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  50000.0         │ [0.5 BTC, 1.2 BTC│ (0.5×50000) +    │   100000 USD     │
    /// │  (USDT/BTC)      │ ] + [0.3 BTC]    │ (1.2×50000) +    │                  │
    /// │                  │                  │ (0.3×50000)      │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  2500.0          │ [1000 ETH,       │ (1000×2500) +    │   3750000 USD    │
    /// │  (USDT/ETH)      │ 500 ETH]         │ (500×2500)       │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  1.0             │ [50000 ADA]      │ 50000 × 1.0      │    50000 USD     │
    /// │  (USDT/ADA)      │                  │                  │                  │
    /// ├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
    /// │  51000.0         │ [] (No orders)   │ 0.0              │      0 USD       │
    /// │  (USDT/BTC)      │                  │                  │                  │
    /// └──────────────────┴──────────────────┴──────────────────┴──────────────────┘
    /// # Returns
    /// * map { k: price level, v: total quote value }
    pub fn get_price_level_total_quote_value(&self) -> BTreeMap<u64, f64> {
        let mut distribution = BTreeMap::new();
        for shard in &self.shards {
            let shard_guard = shard.read();
            for (price, orders) in &shard_guard.tree {
                let price_key = (price.to_f64() * 10000.0) as u64;
                let total_quote_value: f64 = orders
                    .iter()
                    .map(|order| order.remaining * price.to_f64())
                    .sum();
                *distribution.entry(price_key).or_insert(0.0) += total_quote_value;
            }
        }
        distribution
    }
}

/// A specific implementation of the BidPrice type.
impl OrderTreeSharding<BidPrice> {
    pub fn get_all_bid_prices_sorted(&self) -> Vec<BidPrice> {
        let mut all_prices = Vec::new();
        for shard in &self.shards {
            let shard_guard = shard.read();
            for price in shard_guard.tree.keys() {
                all_prices.push(price.clone());
            }
        }
        all_prices.sort();
        all_prices
    }

    pub fn get_all_bid_prices_sorted_desc(&self) -> Vec<BidPrice> {
        self.get_all_bid_prices_sorted()
    }

    pub fn get_all_bid_prices_sorted_asc(&self) -> Vec<BidPrice> {
        let mut prices = self.get_all_bid_prices_sorted();
        prices.reverse();
        prices
    }
}

/// A specific implementation of the AskPrice type.
impl OrderTreeSharding<AskPrice> {
    pub fn get_all_ask_prices_sorted(&self) -> Vec<AskPrice> {
        let mut all_prices = Vec::new();
        for shard in &self.shards {
            let shard_guard = shard.read();
            for price in shard_guard.tree.keys() {
                all_prices.push(price.clone());
            }
        }
        all_prices.sort();
        all_prices
    }

    pub fn get_all_ask_prices_sorted_desc(&self) -> Vec<AskPrice> {
        let mut prices = self.get_all_ask_prices_sorted();
        prices.reverse();
        prices
    }

    pub fn get_all_ask_prices_sorted_asc(&self) -> Vec<AskPrice> {
        self.get_all_ask_prices_sorted()
    }
}
