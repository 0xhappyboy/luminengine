use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam::channel::{Receiver, Sender, unbounded};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::{
    matchengine::{
        slfe::{Slfe, processor::OrderProcessor},
        tool::math::f64_to_price_u64,
    },
    order::{Order, OrderDirection},
    types::{UnifiedError, UnifiedResult},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum StopOrderStatus {
    Pending,
    Triggered,
    Cancelled,
    Executed,
    Expired,
}

#[derive(Debug, Clone)]
pub struct StopOrder {
    pub id: String,
    pub original_order: Order,
    pub stop_price: f64,
    pub status: StopOrderStatus,
    pub created_at: Instant,
    pub triggered_at: Option<Instant>,
    pub expiry_time: Option<Instant>,
}

#[derive(Debug)]
pub struct PriceChangeManager {
    pub stop_orders: Arc<DashMap<String, StopOrder>>,
    pub buy_stop_orders: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    pub sell_stop_orders: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    pub stop_limit_orders: Arc<DashMap<String, StopOrder>>,
    pub buy_stop_limit_orders: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    pub sell_stop_limit_orders: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    pub rx: Receiver<f64>,
    pub tx: Sender<f64>,
    pub is_running: Arc<AtomicBool>,
}

impl PriceChangeManager {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            stop_orders: Arc::new(DashMap::new()),
            buy_stop_orders: Arc::new(RwLock::new(BTreeMap::new())),
            sell_stop_orders: Arc::new(RwLock::new(BTreeMap::new())),
            stop_limit_orders: Arc::new(DashMap::new()),
            buy_stop_limit_orders: Arc::new(RwLock::new(BTreeMap::new())),
            sell_stop_limit_orders: Arc::new(RwLock::new(BTreeMap::new())),
            rx,
            tx,
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn price_listener_task(
        slfe: Arc<Slfe>,
        tx: Sender<f64>,
        is_running: Arc<AtomicBool>,
    ) {
        let mut last_price = 0.0;
        let price_check_interval = Duration::from_micros(100);
        while is_running.load(Ordering::SeqCst) {
            let current_price = slfe.get_current_price();

            if (current_price - last_price).abs() > f64::EPSILON {
                if tx.send(current_price).is_err() {
                    break;
                }
                last_price = current_price;
            }
            tokio::time::sleep(price_check_interval).await;
        }
    }

    pub async fn handle_stop_order(
        stop_orders: Arc<DashMap<String, StopOrder>>,
        buy_stop_orders: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
        sell_stop_orders: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
        price_rx: Receiver<f64>,
        slfe: Arc<Slfe>,
        is_running: Arc<AtomicBool>,
    ) {
        while is_running.load(Ordering::SeqCst) {
            if let Ok(current_price) = price_rx.try_recv() {
                Self::handle_buy_stop_orders(
                    &stop_orders,
                    &buy_stop_orders,
                    current_price,
                    Arc::clone(&slfe),
                )
                .await;
                Self::handle_sell_stop_orders(
                    &stop_orders,
                    &sell_stop_orders,
                    current_price,
                    Arc::clone(&slfe),
                )
                .await;
                Self::clean_expired_orders(&stop_orders, &buy_stop_orders, &sell_stop_orders);
            }
            tokio::time::sleep(Duration::from_micros(50)).await;
        }
    }

    pub async fn handle_stop_limit_order(
        stop_orders: Arc<DashMap<String, StopOrder>>,
        buy_stop_orders: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
        sell_stop_orders: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
        price_rx: Receiver<f64>,
        slfe: Arc<Slfe>,
        is_running: Arc<AtomicBool>,
    ) {
        while is_running.load(Ordering::SeqCst) {
            if let Ok(current_price) = price_rx.try_recv() {
                Self::handle_buy_stop_limit_orders(
                    &stop_orders,
                    &buy_stop_orders,
                    current_price,
                    Arc::clone(&slfe),
                )
                .await;
                Self::handle_sell_stop_limit_orders(
                    &stop_orders,
                    &sell_stop_orders,
                    current_price,
                    Arc::clone(&slfe),
                )
                .await;

                Self::clean_expired_orders(&stop_orders, &buy_stop_orders, &sell_stop_orders);
            }

            tokio::time::sleep(Duration::from_micros(50)).await;
        }
    }

    async fn handle_buy_stop_orders(
        stop_orders: &DashMap<String, StopOrder>,
        buy_stop_orders: &RwLock<BTreeMap<u64, Vec<String>>>,
        current_price: f64,
        slfe: Arc<Slfe>,
    ) {
        let price_key = f64_to_price_u64(current_price);
        let triggered_orders = {
            let buy_orders = buy_stop_orders.read();

            buy_orders
                .range(..=price_key)
                .flat_map(|(_, order_ids)| order_ids.clone())
                .collect::<Vec<String>>()
        };

        for order_id in triggered_orders {
            if let Some(mut stop_order) = stop_orders.get_mut(&order_id) {
                if stop_order.status == StopOrderStatus::Pending {
                    stop_order.status = StopOrderStatus::Triggered;
                    stop_order.triggered_at = Some(Instant::now());

                    let mut order = stop_order.original_order.clone();
                    order.id = format!("triggered_{}", order.id);
                    order.order_type = crate::order::OrderType::Market;
                    OrderProcessor::handle_new_order(slfe.clone(), order);
                }
            }
        }
    }

    async fn handle_sell_stop_orders(
        stop_orders: &DashMap<String, StopOrder>,
        sell_stop_orders: &RwLock<BTreeMap<u64, Vec<String>>>,
        current_price: f64,
        slfe: Arc<Slfe>,
    ) {
        let price_key = f64_to_price_u64(current_price);
        let triggered_orders = {
            let sell_orders = sell_stop_orders.read();

            sell_orders
                .range(price_key..)
                .flat_map(|(_, order_ids)| order_ids.clone())
                .collect::<Vec<String>>()
        };
        for order_id in triggered_orders {
            if let Some(mut stop_order) = stop_orders.get_mut(&order_id) {
                if stop_order.status == StopOrderStatus::Pending {
                    stop_order.status = StopOrderStatus::Triggered;
                    stop_order.triggered_at = Some(Instant::now());

                    let mut order = stop_order.original_order.clone();
                    order.id = format!("triggered_{}", order.id);
                    order.order_type = crate::order::OrderType::Market;
                    OrderProcessor::handle_new_order(slfe.clone(), order);
                }
            }
        }
    }

    async fn handle_buy_stop_limit_orders(
        stop_orders: &DashMap<String, StopOrder>,
        buy_stop_orders: &RwLock<BTreeMap<u64, Vec<String>>>,
        current_price: f64,
        slfe: Arc<Slfe>,
    ) {
        let price_key = f64_to_price_u64(current_price);
        let triggered_orders = {
            let buy_orders = buy_stop_orders.read();

            buy_orders
                .range(..=price_key)
                .flat_map(|(_, order_ids)| order_ids.clone())
                .collect::<Vec<String>>()
        };
        for order_id in triggered_orders {
            if let Some(mut stop_order) = stop_orders.get_mut(&order_id) {
                if stop_order.status == StopOrderStatus::Pending {
                    stop_order.status = StopOrderStatus::Triggered;
                    stop_order.triggered_at = Some(Instant::now());

                    let mut order = stop_order.original_order.clone();
                    order.id = format!("triggered_{}", order.id);
                    order.order_type = crate::order::OrderType::Limit;
                    OrderProcessor::handle_new_order(slfe.clone(), order);
                }
            }
        }
    }

    async fn handle_sell_stop_limit_orders(
        stop_orders: &DashMap<String, StopOrder>,
        sell_stop_orders: &RwLock<BTreeMap<u64, Vec<String>>>,
        current_price: f64,
        slfe: Arc<Slfe>,
    ) {
        let price_key = f64_to_price_u64(current_price);
        let triggered_orders = {
            let sell_orders = sell_stop_orders.read();
            sell_orders
                .range(price_key..)
                .flat_map(|(_, order_ids)| order_ids.clone())
                .collect::<Vec<String>>()
        };
        for order_id in triggered_orders {
            if let Some(mut stop_order) = stop_orders.get_mut(&order_id) {
                if stop_order.status == StopOrderStatus::Pending {
                    stop_order.status = StopOrderStatus::Triggered;
                    stop_order.triggered_at = Some(Instant::now());

                    let mut order = stop_order.original_order.clone();
                    order.id = format!("triggered_{}", order.id);
                    order.order_type = crate::order::OrderType::Limit;
                    OrderProcessor::handle_new_order(slfe.clone(), order);
                }
            }
        }
    }

    /// clean expired orders
    fn clean_expired_orders(
        stop_orders: &DashMap<String, StopOrder>,
        buy_stop_orders: &RwLock<BTreeMap<u64, Vec<String>>>,
        sell_stop_orders: &RwLock<BTreeMap<u64, Vec<String>>>,
    ) {
        let now = Instant::now();
        let mut expired_ids = Vec::new();
        for entry in stop_orders.iter() {
            let stop_order = entry.value();
            if let Some(expiry) = stop_order.expiry_time {
                if now >= expiry && stop_order.status == StopOrderStatus::Pending {
                    expired_ids.push(stop_order.id.clone());
                }
            }
        }
        for order_id in expired_ids {
            if let Some(stop_order) = stop_orders.remove(&order_id) {
                let (_, stop_order) = stop_order;
                Self::remove_from_price_index(&stop_order, &buy_stop_orders, &sell_stop_orders);
            }
        }
    }

    /// add stop order
    pub fn add_stop_order(
        &self,
        original_order: Order,
        stop_price: f64,
        is_limit: bool,
        limit_price: Option<f64>,
        expiry_seconds: Option<u64>,
    ) -> UnifiedResult<String> {
        if stop_price <= 0.0 {
            return Err(UnifiedError::InvalidStopPrice(format!(
                "Invalid stop price: {}",
                stop_price
            )));
        }
        if is_limit {
            if let Some(limit_price) = limit_price {
                if limit_price <= 0.0 {
                    return Err(UnifiedError::InvalidLimitPrice(format!(
                        "Invalid limit price: {}",
                        limit_price
                    )));
                }
            } else {
                return Err(UnifiedError::InvalidLimitPrice(
                    "Limit price is required for stop-limit orders".to_string(),
                ));
            }
        }
        let mut final_order = original_order;
        if is_limit {
            final_order.price = limit_price.unwrap();
            final_order.order_type = crate::order::OrderType::Limit;
        } else {
            final_order.order_type = crate::order::OrderType::Market;
        }
        let stop_order = StopOrder {
            id: format!(
                "{}_{}_{}",
                if is_limit { "stop_limit" } else { "stop" },
                final_order.id,
                Instant::now().elapsed().as_nanos()
            ),
            original_order: final_order,
            stop_price,
            status: StopOrderStatus::Pending,
            created_at: Instant::now(),
            triggered_at: None,
            expiry_time: expiry_seconds.map(|secs| Instant::now() + Duration::from_secs(secs)),
        };
        let order_id = stop_order.id.clone();
        if is_limit {
            self.stop_limit_orders
                .insert(order_id.clone(), stop_order.clone());
            self.add_to_stop_limit_price_index(&stop_order);
        } else {
            self.stop_orders
                .insert(order_id.clone(), stop_order.clone());
            self.add_to_stop_price_index(&stop_order);
        }
        Ok(order_id)
    }

    pub fn cancel_stop_order(&self, order_id: &str) -> UnifiedResult<bool> {
        if let Some((_, stop_order)) = self.stop_orders.remove(order_id) {
            self.remove_from_stop_price_index(&stop_order);
            return Ok(true);
        }
        if let Some((_, stop_order)) = self.stop_limit_orders.remove(order_id) {
            self.remove_from_stop_limit_price_index(&stop_order);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn set_stop_order(
        &self,
        order_id: &str,
        new_stop_price: f64,
        new_limit_price: Option<f64>,
    ) -> UnifiedResult<bool> {
        if new_stop_price <= 0.0 {
            return Err(UnifiedError::InvalidStopPrice(format!(
                "Invalid stop price: {}",
                new_stop_price
            )));
        }
        if let Some(mut entry) = self.stop_orders.get_mut(order_id) {
            if entry.status != StopOrderStatus::Pending {
                return Err(UnifiedError::OrderNotModifiable(
                    "Only pending stop orders can be modified".to_string(),
                ));
            }
            let old_stop_price = entry.stop_price;
            entry.stop_price = new_stop_price;
            self.remove_from_stop_price_index(&StopOrder {
                id: order_id.to_string(),
                stop_price: old_stop_price,
                ..entry.clone()
            });
            self.add_to_stop_price_index(&entry);
            return Ok(true);
        }
        if let Some(mut entry) = self.stop_limit_orders.get_mut(order_id) {
            if entry.status != StopOrderStatus::Pending {
                return Err(UnifiedError::OrderNotModifiable(
                    "Only pending stop orders can be modified".to_string(),
                ));
            }
            let old_stop_price = entry.stop_price;
            entry.stop_price = new_stop_price;

            if let Some(limit_price) = new_limit_price {
                if limit_price <= 0.0 {
                    return Err(UnifiedError::InvalidLimitPrice(format!(
                        "Invalid limit price: {}",
                        limit_price
                    )));
                }
                entry.original_order.price = limit_price;
            }
            self.remove_from_stop_limit_price_index(&StopOrder {
                id: order_id.to_string(),
                stop_price: old_stop_price,
                ..entry.clone()
            });
            self.add_to_stop_limit_price_index(&entry);

            return Ok(true);
        }
        Ok(false)
    }

    fn add_to_stop_price_index(&self, stop_order: &StopOrder) {
        let price_key = f64_to_price_u64(stop_order.stop_price);

        match stop_order.original_order.direction {
            OrderDirection::Buy => {
                let mut buy_orders = self.buy_stop_orders.write();
                buy_orders
                    .entry(price_key)
                    .or_insert_with(Vec::new)
                    .push(stop_order.id.clone());
            }
            OrderDirection::Sell => {
                let mut sell_orders = self.sell_stop_orders.write();
                sell_orders
                    .entry(price_key)
                    .or_insert_with(Vec::new)
                    .push(stop_order.id.clone());
            }
            OrderDirection::None => {}
        }
    }

    fn add_to_stop_limit_price_index(&self, stop_order: &StopOrder) {
        let price_key = f64_to_price_u64(stop_order.stop_price);

        match stop_order.original_order.direction {
            OrderDirection::Buy => {
                let mut buy_orders = self.buy_stop_limit_orders.write();
                buy_orders
                    .entry(price_key)
                    .or_insert_with(Vec::new)
                    .push(stop_order.id.clone());
            }
            OrderDirection::Sell => {
                let mut sell_orders = self.sell_stop_limit_orders.write();
                sell_orders
                    .entry(price_key)
                    .or_insert_with(Vec::new)
                    .push(stop_order.id.clone());
            }
            OrderDirection::None => {}
        }
    }

    fn remove_from_stop_price_index(&self, stop_order: &StopOrder) {
        let price_key = f64_to_price_u64(stop_order.stop_price);

        match stop_order.original_order.direction {
            OrderDirection::Buy => {
                let mut buy_orders = self.buy_stop_orders.write();
                if let Some(order_ids) = buy_orders.get_mut(&price_key) {
                    order_ids.retain(|id| id != &stop_order.id);
                    if order_ids.is_empty() {
                        buy_orders.remove(&price_key);
                    }
                }
            }
            OrderDirection::Sell => {
                let mut sell_orders = self.sell_stop_orders.write();
                if let Some(order_ids) = sell_orders.get_mut(&price_key) {
                    order_ids.retain(|id| id != &stop_order.id);
                    if order_ids.is_empty() {
                        sell_orders.remove(&price_key);
                    }
                }
            }
            OrderDirection::None => {}
        }
    }

    fn remove_from_stop_limit_price_index(&self, stop_order: &StopOrder) {
        let price_key = f64_to_price_u64(stop_order.stop_price);

        match stop_order.original_order.direction {
            OrderDirection::Buy => {
                let mut buy_orders = self.buy_stop_limit_orders.write();
                if let Some(order_ids) = buy_orders.get_mut(&price_key) {
                    order_ids.retain(|id| id != &stop_order.id);
                    if order_ids.is_empty() {
                        buy_orders.remove(&price_key);
                    }
                }
            }
            OrderDirection::Sell => {
                let mut sell_orders = self.sell_stop_limit_orders.write();
                if let Some(order_ids) = sell_orders.get_mut(&price_key) {
                    order_ids.retain(|id| id != &stop_order.id);
                    if order_ids.is_empty() {
                        sell_orders.remove(&price_key);
                    }
                }
            }
            OrderDirection::None => {}
        }
    }

    fn add_to_price_index(&self, stop_order: &StopOrder) {
        let price_key = f64_to_price_u64(stop_order.stop_price);

        match stop_order.original_order.direction {
            OrderDirection::Buy => {
                let mut buy_orders = self.buy_stop_orders.write();
                buy_orders
                    .entry(price_key)
                    .or_insert_with(Vec::new)
                    .push(stop_order.id.clone());
            }
            OrderDirection::Sell => {
                let mut sell_orders = self.sell_stop_orders.write();
                sell_orders
                    .entry(price_key)
                    .or_insert_with(Vec::new)
                    .push(stop_order.id.clone());
            }
            OrderDirection::None => {}
        }
    }

    fn remove_from_price_index(
        stop_order: &StopOrder,
        buy_stop_orders: &RwLock<BTreeMap<u64, Vec<String>>>,
        sell_stop_orders: &RwLock<BTreeMap<u64, Vec<String>>>,
    ) {
        let price_key = f64_to_price_u64(stop_order.stop_price);

        match stop_order.original_order.direction {
            OrderDirection::Buy => {
                let mut buy_orders = buy_stop_orders.write();
                if let Some(order_ids) = buy_orders.get_mut(&price_key) {
                    order_ids.retain(|id| id != &stop_order.id);
                    if order_ids.is_empty() {
                        buy_orders.remove(&price_key);
                    }
                }
            }
            OrderDirection::Sell => {
                let mut sell_orders = sell_stop_orders.write();
                if let Some(order_ids) = sell_orders.get_mut(&price_key) {
                    order_ids.retain(|id| id != &stop_order.id);
                    if order_ids.is_empty() {
                        sell_orders.remove(&price_key);
                    }
                }
            }
            OrderDirection::None => {}
        }
    }

    pub fn get_stop_order_status(&self, order_id: &str) -> Option<StopOrderStatus> {
        self.stop_orders
            .get(order_id)
            .map(|entry| entry.status.clone())
    }

    pub fn get_total_stop_orders(&self) -> usize {
        self.stop_orders.len()
    }

    pub fn get_pending_stop_orders(&self) -> usize {
        self.stop_orders
            .iter()
            .filter(|entry| entry.status == StopOrderStatus::Pending)
            .count()
    }
}
