/// Iceberg Order Management
///
/// This module provides a method for managing iceberg orders in a high frequency trading environment.
///
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam::channel::{Receiver, Sender, bounded};
use dashmap::DashMap;

use crate::{
    matchengine::slfe::{Slfe, processor::OrderProcessor},
    order::{Order, OrderDirection, OrderStatus, OrderType},
    types::{UnifiedError, UnifiedResult},
};

#[derive(Debug, Clone)]
pub struct IcebergOrderTier {
    pub price: f64,
    pub display_quantity: f64,
    pub filled_quantity: f64,
    pub display_order_id: Option<String>,
    pub is_active: bool,
    pub is_completed: bool,
}

#[derive(Debug, Clone)]
pub struct IcebergOrderStatus {
    pub original_order_id: String,
    pub symbol: String,
    pub direction: OrderDirection,
    pub total_quantity: f64,
    pub remaining_total: f64,
    pub tiers: Vec<IcebergOrderTier>,
    pub current_tier_index: usize,
    pub last_refresh: Instant,
    pub is_completed: bool,
    pub is_cancelled: bool,
    pub config: IcebergOrderConfig,
}

#[derive(Debug, Clone)]
pub struct IcebergOrderConfig {
    pub display_quantity: f64,
    pub price_increment: f64,
    pub max_tiers: usize,
}

impl Default for IcebergOrderConfig {
    fn default() -> Self {
        Self {
            display_quantity: 1.0,
            price_increment: 0.1,
            max_tiers: 5,
        }
    }
}

#[derive(Debug)]
pub enum IcebergOrderEvent {
    Create {
        order: Order,
        config: IcebergOrderConfig,
    },
    Cancel {
        order_id: String,
    },
    TierFilled {
        display_order_id: String,
        filled_quantity: f64,
    },
    TierCompleted {
        display_order_id: String,
        filled_quantity: f64,
    },
    Shutdown,
}

#[derive(Debug)]
pub struct IcebergCachePool {
    active_orders: DashMap<String, IcebergOrderStatus>,
    display_to_iceberg: DashMap<String, String>,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

impl IcebergCachePool {
    fn new() -> Self {
        Self {
            active_orders: DashMap::new(),
            display_to_iceberg: DashMap::new(),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }

    pub fn add_iceberg_order(&self, order_state: IcebergOrderStatus) {
        let order_id = order_state.original_order_id.clone();
        self.active_orders.insert(order_id, order_state);
    }

    pub fn get_iceberg_order(&self, order_id: &str) -> Option<IcebergOrderStatus> {
        match self.active_orders.get(order_id) {
            Some(state) => {
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
                Some(state.clone())
            }
            None => {
                self.cache_misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    fn get_iceberg_order_mut(
        &self,
        order_id: &str,
    ) -> Option<impl std::ops::DerefMut<Target = IcebergOrderStatus> + '_> {
        self.active_orders.get_mut(order_id)
    }

    fn update_iceberg_order(&self, order_id: &str, new_state: IcebergOrderStatus) {
        self.active_orders.insert(order_id.to_string(), new_state);
    }

    fn remove_iceberg_order(&self, order_id: &str) -> Option<IcebergOrderStatus> {
        if let Some((_, state)) = self.active_orders.remove(order_id) {
            for tier in &state.tiers {
                if let Some(display_id) = &tier.display_order_id {
                    self.display_to_iceberg.remove(display_id);
                }
            }
            Some(state)
        } else {
            None
        }
    }

    fn add_display_mapping(&self, display_order_id: String, iceberg_order_id: String) {
        self.display_to_iceberg
            .insert(display_order_id, iceberg_order_id);
    }

    fn get_iceberg_order_for_display(&self, display_order_id: &str) -> Option<String> {
        self.display_to_iceberg
            .get(display_order_id)
            .map(|id| id.value().clone())
    }

    fn cleanup_completed_orders(&self) {
        let completed_ids: Vec<String> = self
            .active_orders
            .iter()
            .filter(|entry| entry.value().is_completed || entry.value().is_cancelled)
            .map(|entry| entry.key().clone())
            .collect();

        for order_id in completed_ids {
            self.remove_iceberg_order(&order_id);
        }
    }

    fn get_stats(&self) -> (u64, u64) {
        (
            self.cache_hits.load(Ordering::Relaxed),
            self.cache_misses.load(Ordering::Relaxed),
        )
    }
}

#[derive(Debug)]
pub struct IcebergOrderManager {
    pub event_tx: Sender<IcebergOrderEvent>,
    event_rx: Receiver<IcebergOrderEvent>,
    is_running: Arc<AtomicBool>,
    pub cache_pool: Arc<IcebergCachePool>,
}

impl IcebergOrderManager {
    pub fn new() -> Self {
        let (event_tx, event_rx) = bounded(10_000);
        Self {
            event_tx,
            event_rx,
            is_running: Arc::new(AtomicBool::new(false)),
            cache_pool: Arc::new(IcebergCachePool::new()),
        }
    }

    pub async fn start_iceberg_manager(self: Arc<Self>, engine: Arc<Slfe>) {
        self.is_running.store(true, Ordering::Relaxed);
        let mut event_batch = Vec::<IcebergOrderEvent>::with_capacity(100);
        let mut last_cleanup_time = Instant::now();
        let cleanup_interval = Duration::from_secs(30); // Clean up every 30 seconds
        while self.is_running.load(Ordering::Relaxed) {
            while let Ok(event) = self.event_rx.try_recv() {
                event_batch.push(event);
                if event_batch.len() >= 100 {
                    self.handle_event_batch(&event_batch, &engine);
                    event_batch.clear();
                }
            }
            if !event_batch.is_empty() {
                self.handle_event_batch(&event_batch, &engine);
                event_batch.clear();
            }
            if last_cleanup_time.elapsed() >= cleanup_interval {
                self.cache_pool.cleanup_completed_orders();
                last_cleanup_time = Instant::now();
            }
            std::thread::sleep(Duration::from_micros(100));
        }
    }

    fn handle_event_batch(&self, events: &[IcebergOrderEvent], engine: &Arc<Slfe>) {
        let start_time = Instant::now();
        let mut processed_count = 0;
        for event in events {
            Self::handle_single_event(event, &self.cache_pool, engine);
            processed_count += 1;
        }
        let elapsed = start_time.elapsed();
        if elapsed > Duration::from_millis(10) {
            println!(
                "Iceberg Order Manager Batch Processing: Processed {} events, time taken: {:?}",
                processed_count, elapsed
            );
        }
    }

    fn handle_single_event(
        event: &IcebergOrderEvent,
        cache_pool: &Arc<IcebergCachePool>,
        engine: &Arc<Slfe>,
    ) {
        match event {
            IcebergOrderEvent::Create { order, config } => {
                Self::handle_create_order(order.clone(), config.clone(), cache_pool, engine);
            }
            IcebergOrderEvent::Cancel { order_id } => {
                Self::handle_cancel_order(order_id, cache_pool, engine.clone());
            }
            IcebergOrderEvent::TierFilled {
                display_order_id,
                filled_quantity,
            } => {
                Self::handle_tier_filled(display_order_id, *filled_quantity, cache_pool);
            }
            IcebergOrderEvent::TierCompleted {
                display_order_id,
                filled_quantity,
            } => {
                Self::handle_tier_completed(display_order_id, *filled_quantity, cache_pool, engine);
            }
            IcebergOrderEvent::Shutdown => {
                // close event
            }
        }
    }

    pub fn stop_manager(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }

    fn handle_create_order(
        order: Order,
        config: IcebergOrderConfig,
        cache_pool: &Arc<IcebergCachePool>,
        engine: &Arc<Slfe>,
    ) -> UnifiedResult<String> {
        if order.order_type != OrderType::Iceberg {
            return Err(UnifiedError::IcebergOrderError(
                "Order Type Error".to_string(),
            ));
        }
        let tiers = Self::calculate_tiers(&order, &config);
        let iceberg_state = IcebergOrderStatus {
            original_order_id: order.id.clone(),
            symbol: order.symbol.clone(),
            direction: order.direction,
            total_quantity: order.quantity,
            remaining_total: order.quantity,
            tiers,
            current_tier_index: 0,
            last_refresh: Instant::now(),
            is_completed: false,
            is_cancelled: false,
            config: config.clone(),
        };
        cache_pool.add_iceberg_order(iceberg_state);
        Self::activate_tier(&order.id, 0, cache_pool, engine)?;
        Ok(order.id)
    }

    fn calculate_tiers(order: &Order, config: &IcebergOrderConfig) -> Vec<IcebergOrderTier> {
        vec![IcebergOrderTier {
            price: order.price,
            display_quantity: config.display_quantity.min(order.quantity),
            filled_quantity: 0.0,
            display_order_id: None,
            is_active: false,
            is_completed: false,
        }]
    }

    fn activate_tier(
        iceberg_order_id: &str,
        tier_index: usize,
        cache_pool: &Arc<IcebergCachePool>,
        slfe: &Arc<Slfe>,
    ) -> UnifiedResult<String> {
        let state = match cache_pool.get_iceberg_order(iceberg_order_id) {
            Some(state) => state,
            None => {
                return Err(UnifiedError::IcebergOrderError(
                    "Order does not exist".to_string(),
                ));
            }
        };
        if tier_index >= state.tiers.len() {
            return Err(UnifiedError::IcebergOrderError(
                "Level index out of range".to_string(),
            ));
        }
        let tier = &state.tiers[tier_index];
        if tier.display_quantity <= 0.0 || state.remaining_total <= 0.0 {
            return Err(UnifiedError::IcebergOrderError(
                "No Tradable quantity".to_string(),
            ));
        }

        let mut state_mut = match cache_pool.get_iceberg_order_mut(iceberg_order_id) {
            Some(state) => state,
            None => {
                return Err(UnifiedError::IcebergOrderError(
                    "Order does not exist".to_string(),
                ));
            }
        };

        if tier_index >= state_mut.tiers.len() {
            return Err(UnifiedError::IcebergOrderError(
                "Level index out of range".to_string(),
            ));
        }

        let state_symbol = state_mut.symbol.clone();
        let state_direction = state_mut.direction.clone();
        let state_remaining_total = state_mut.remaining_total;
        let tier_mut = &mut state_mut.tiers[tier_index];

        if tier_mut.display_quantity <= 0.0 || state_remaining_total <= 0.0 {
            return Err(UnifiedError::IcebergOrderError(
                "No Tradable quantity".to_string(),
            ));
        }

        let display_order_id = format!("{}_{}", iceberg_order_id, tier_index);
        let display_order = Order {
            id: display_order_id.clone(),
            symbol: state_symbol,
            price: tier_mut.price,
            direction: state_direction,
            quantity: tier_mut.display_quantity,
            remaining: tier_mut.display_quantity,
            filled: 0.0,
            crt_time: chrono::Utc::now().to_string(),
            status: OrderStatus::Pending,
            expiry: None,
            order_type: OrderType::Limit,
            ex: None,
        };

        let slfe_clone = slfe.clone();
        let order_clone = display_order.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            match rt.block_on(OrderProcessor::handle_new_order(slfe_clone, order_clone)) {
                Ok(s) => {
                    // .....
                }
                Err(e) => {}
            }
        });
        tier_mut.is_active = true;
        tier_mut.display_order_id = Some(display_order.id.clone());
        state_mut.current_tier_index = tier_index;
        state_mut.last_refresh = Instant::now();
        cache_pool.add_display_mapping(display_order.id, iceberg_order_id.to_string());
        Ok("ok".to_string())
    }

    fn create_next_tier(
        current_state: &IcebergOrderStatus,
        config: &IcebergOrderConfig,
        next_tier_index: usize,
    ) -> IcebergOrderTier {
        let current_tier_price = current_state.tiers.last().unwrap().price;
        let next_tier_price = match current_state.direction {
            OrderDirection::Buy => current_tier_price - config.price_increment,
            OrderDirection::Sell => current_tier_price + config.price_increment,
            OrderDirection::None => current_tier_price,
        };
        let next_tier_quantity = config.display_quantity.min(current_state.remaining_total);
        IcebergOrderTier {
            price: next_tier_price,
            display_quantity: next_tier_quantity,
            filled_quantity: 0.0,
            display_order_id: None,
            is_active: false,
            is_completed: false,
        }
    }

    fn handle_tier_filled(
        display_order_id: &str,
        filled_quantity: f64,
        cache_pool: &Arc<IcebergCachePool>,
    ) {
        let iceberg_order_id = match cache_pool.get_iceberg_order_for_display(display_order_id) {
            Some(id) => id,
            None => {
                return;
            }
        };

        if let Some(mut state) = cache_pool.get_iceberg_order_mut(&iceberg_order_id) {
            let tier_index =
                match state.tiers.iter().position(|t| {
                    t.display_order_id.as_ref() == Some(&display_order_id.to_string())
                }) {
                    Some(index) => index,
                    None => {
                        return;
                    }
                };

            if tier_index >= state.tiers.len() {
                return;
            }

            state.remaining_total -= filled_quantity;
            let tier = &mut state.tiers[tier_index];
            tier.filled_quantity += filled_quantity;

            if tier.filled_quantity >= tier.display_quantity {
                tier.is_completed = true;
            }

            if state.remaining_total <= 0.0 {
                state.is_completed = true;
            }
        } else {
            // Order does not exist...
        }
    }

    fn handle_tier_completed(
        display_order_id: &str,
        filled_quantity: f64,
        cache_pool: &Arc<IcebergCachePool>,
        engine: &Arc<Slfe>,
    ) {
        let iceberg_order_id = match cache_pool.get_iceberg_order_for_display(display_order_id) {
            Some(id) => id,
            None => {
                return;
            }
        };

        let (tier_index, config, remaining_total) = {
            let state = match cache_pool.get_iceberg_order(&iceberg_order_id) {
                Some(state) => state,
                None => {
                    return;
                }
            };

            let tier_index = state
                .tiers
                .iter()
                .position(|t| t.display_order_id.as_ref() == Some(&display_order_id.to_string()));

            (tier_index, state.config.clone(), state.remaining_total)
        };

        let tier_index = match tier_index {
            Some(index) => index,
            None => {
                return;
            }
        };

        if let Some(mut state) = cache_pool.get_iceberg_order_mut(&iceberg_order_id) {
            if tier_index < state.tiers.len() {
                let tier = &mut state.tiers[tier_index];
                tier.filled_quantity = filled_quantity;
                tier.is_completed = true;
                tier.is_active = false;
            }
        }

        if remaining_total > 0.0 {
            let next_tier_index = tier_index + 1;
            Self::create_and_activate_next_tier(
                &iceberg_order_id,
                next_tier_index,
                cache_pool,
                engine,
                &config,
            );
        } else {
            if let Some(mut state) = cache_pool.get_iceberg_order_mut(&iceberg_order_id) {
                state.is_completed = true;
            }
            // Remove completed orders from the cache pool
            cache_pool.remove_iceberg_order(&iceberg_order_id);
        }
    }

    fn create_and_activate_next_tier(
        iceberg_order_id: &str,
        next_tier_index: usize,
        cache_pool: &Arc<IcebergCachePool>,
        engine: &Arc<Slfe>,
        config: &IcebergOrderConfig,
    ) {
        let current_state = match cache_pool.get_iceberg_order(iceberg_order_id) {
            Some(state) => state,
            None => return,
        };
        if next_tier_index >= config.max_tiers {
            return;
        }
        let next_tier = Self::create_next_tier(&current_state, config, next_tier_index);
        if let Some(mut state) = cache_pool.get_iceberg_order_mut(iceberg_order_id) {
            state.tiers.push(next_tier);
            // manual release
            drop(state);
            if let Err(e) =
                Self::activate_tier(iceberg_order_id, next_tier_index, cache_pool, engine)
            {
                // Failed to activate the next level ..
            }
        }
    }

    async fn handle_cancel_order(
        order_id: &str,
        cache_pool: &Arc<IcebergCachePool>,
        slfe: Arc<Slfe>,
    ) -> UnifiedResult<String> {
        let state = match cache_pool.get_iceberg_order(order_id) {
            Some(state) => state,
            None => {
                return Err(UnifiedError::IcebergOrderError(
                    "Order does not exist".to_string(),
                ));
            }
        };
        for tier in &state.tiers {
            if let Some(display_id) = &tier.display_order_id {
                if tier.is_active && !tier.is_completed {
                    // cancel order
                    OrderProcessor::handle_cancel_order(slfe.as_ref(), &display_id).await;
                }
            }
        }
        cache_pool.remove_iceberg_order(order_id);
        Ok("ok".to_string())
    }
}

impl Drop for IcebergOrderManager {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::Relaxed);
        let _ = self.event_tx.send(IcebergOrderEvent::Shutdown);
    }
}
