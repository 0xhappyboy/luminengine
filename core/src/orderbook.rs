use crate::{
    matchengine::{
        MatchEngineConfig,
        matcher::Matcher,
        slfe::{
            Slfe,
            manager::{price_change::StopOrderStatus, status::SlfeStatus},
        },
    },
    order::Order,
    target::Target,
};
use dashmap::DashMap;
use lazy_static::lazy_static;
use parking_lot::RwLock;
use std::sync::Arc;

lazy_static! {
    static ref ORDER_BOOKS: DashMap<Target, Arc<RwLock<OrderBook>>> = DashMap::new();
}

/// order source channel enumeration
#[derive(Debug, Clone)]
pub enum OrderSourceChannel {
    Http,
    Tcp,
    Rcp,
}

/// Orderbook collection type.
#[derive(Debug, Clone, Default)]
pub struct OrderBooks;

impl OrderBooks {
    pub fn contains_symbol(symbol: String) -> bool {
        // self.orderbooks.contains_key(&symbol)
        ORDER_BOOKS.contains_key(&Target::new(symbol))
    }
    pub fn get_orderbook_by_symbol(symbol: String) -> Option<Arc<RwLock<OrderBook>>> {
        if OrderBooks::contains_symbol(symbol.clone()) {
            Some(Arc::clone(
                &ORDER_BOOKS.get(&Target { symbol: symbol }).unwrap(),
            ))
        } else {
            None
        }
    }
    pub fn order_num() -> u64 {
        ORDER_BOOKS.len().try_into().unwrap()
    }
    pub fn insert(symbol: String, orderbook: Arc<RwLock<OrderBook>>) -> Result<String, String> {
        if !OrderBooks::contains_symbol(symbol.clone()) {
            ORDER_BOOKS.insert(
                Target {
                    symbol: symbol.clone(),
                },
                orderbook,
            );
            ORDER_BOOKS
                .get(&Target {
                    symbol: symbol.clone(),
                })
                .unwrap()
                .read()
                .enble_matcher(Matcher::new());
            Ok("new order book added successfully".to_string())
        } else {
            Err("symbol already exists".to_string())
        }
    }
}

#[derive(Debug)]
/// border book
pub struct OrderBook {
    pub target: Arc<RwLock<Target>>,
    pub engine: Arc<Slfe>,
}

impl OrderBook {
    pub fn new(target: Target) -> Self {
        Self {
            target: Arc::new(RwLock::new(target.clone())),
            engine: Arc::new(Slfe::new(Some(MatchEngineConfig::default()))),
        }
    }
    // enble matcher engine
    pub fn enble_matcher(&self, matcher: Matcher) {
        let symbol = self.target.read().symbol.clone();
        tokio::spawn(matcher.match_order(symbol));
    }
    /// push order specific implementation logic.
    pub async fn push_order(&mut self, order: Order) {
        let _ = self.engine.clone().add_order(order).await;
    }
    /// matching order
    pub fn matching_order(&self) {}
    /// matching trading
    pub fn matching_trading(&self) {}
    /// order book storage
    pub fn storage() {}

    pub async fn add_order(&self, order: Order) -> crate::types::UnifiedResult<String> {
        self.engine.clone().add_order(order).await
    }

    pub async fn cancel_order(&self, order_id: &str) {
        self.engine.clone().cancel_order(order_id).await;
    }

    pub fn add_stop_order(
        &self,
        original_order: Order,
        stop_price: f64,
        expiry_seconds: Option<u64>,
    ) -> crate::types::UnifiedResult<String> {
        self.engine
            .clone()
            .add_stop_order(original_order, stop_price, expiry_seconds)
    }

    pub fn add_stop_limit_order(
        &self,
        original_order: Order,
        stop_price: f64,
        limit_price: f64,
        expiry_seconds: Option<u64>,
    ) -> crate::types::UnifiedResult<String> {
        self.engine.clone().add_stop_limit_order(
            original_order,
            stop_price,
            limit_price,
            expiry_seconds,
        )
    }

    pub fn cancel_stop_order(&self, order_id: &str) -> crate::types::UnifiedResult<bool> {
        self.engine.clone().cancel_stop_order(order_id)
    }

    pub fn modify_stop_order(
        &self,
        order_id: &str,
        new_stop_price: f64,
        new_limit_price: Option<f64>,
    ) -> crate::types::UnifiedResult<bool> {
        self.engine
            .clone()
            .modify_stop_order(order_id, new_stop_price, new_limit_price)
    }

    pub fn get_gtc_order(&self, order_id: &str) -> Option<Order> {
        self.engine.clone().get_gtc_order(order_id)
    }

    pub fn get_active_gtc_orders(&self) -> Vec<Order> {
        self.engine.clone().get_active_gtc_orders()
    }

    pub async fn check_ioc_feasibility(&self, order: &Order) -> (bool, f64) {
        self.engine.clone().check_ioc_feasibility(order).await
    }

    pub fn get_stop_order_status(&self, order_id: &str) -> Option<StopOrderStatus> {
        self.engine.clone().get_stop_order_status(order_id)
    }

    pub fn get_engine_stats(&self) -> Arc<RwLock<SlfeStatus>> {
        self.engine.clone().get_engine_stats()
    }

    pub fn get_order_location(
        &self,
        order_id: &str,
    ) -> Option<crate::matchengine::slfe::sharding::OrderLocation> {
        self.engine.clone().get_order_location(order_id)
    }

    pub async fn batch_cancel_orders(&self, order_ids: &[&str]) {
        self.engine.clone().batch_cancel_orders(order_ids).await;
    }

    pub fn reload_pending_gtc_orders(&self) -> crate::types::UnifiedResult<Vec<Order>> {
        self.engine.clone().reload_pending_gtc_orders()
    }

    pub fn get_total_active_orders(&self) -> usize {
        self.engine.clone().get_total_active_orders()
    }

    pub async fn cleanup_expired_orders(&self) -> crate::types::UnifiedResult<usize> {
        self.engine.clone().cleanup_expired_orders().await
    }

    pub fn trigger_immediate_match(&self) -> crate::types::UnifiedResult<String> {
        self.engine.clone().trigger_immediate_match()
    }

    pub fn get_engine_config(&self) -> crate::matchengine::MatchEngineConfig {
        self.engine.get_config()
    }

    pub fn update_engine_config(&self, new_config: crate::matchengine::MatchEngineConfig) {
        self.engine.clone().update_config(new_config);
    }

    pub async fn get_market_depth_snapshot(
        &self,
        levels: Option<usize>,
    ) -> crate::market::MarketDepthSnapshot {
        self.engine.clone().get_market_depth_snapshot(levels).await
    }

    pub fn get_current_price(&self) -> f64 {
        self.engine.clone().get_current_price()
    }

    pub fn get_last_match_price(&self) -> f64 {
        self.engine.clone().get_last_match_price()
    }

    pub fn get_mid_price(&self) -> f64 {
        self.engine.clone().get_mid_price()
    }

    pub fn get_spread(&self) -> f64 {
        self.engine.clone().calculate_spread()
    }

    pub async fn start_engine(&self) -> crate::types::UnifiedResult<String> {
        self.engine.clone().start_engine().await
    }

    pub fn get_target(&self) -> Target {
        self.target.read().clone()
    }

    pub async fn get_order_count_by_price(
        &self,
        price: f64,
        direction: crate::order::OrderDirection,
    ) -> usize {
        self.engine.get_order_count_by_price(price, direction).await
    }

    pub async fn get_total_base_unit_by_price(
        &self,
        price: f64,
        direction: crate::order::OrderDirection,
    ) -> f64 {
        self.engine
            .get_total_base_unit_by_price(price, direction)
            .await
    }

    pub async fn get_total_quote_value_by_price(
        &self,
        price: f64,
        direction: crate::order::OrderDirection,
    ) -> f64 {
        self.engine
            .get_total_quote_unit_value(price, direction)
            .await
    }

    pub async fn get_total_order_count_by_direction(
        &self,
        direction: Option<crate::order::OrderDirection>,
    ) -> usize {
        self.engine
            .get_total_order_count_by_direction(direction)
            .await
    }
}
