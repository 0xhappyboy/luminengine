use crossbeam::channel::{Receiver, Sender, unbounded};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    order::{Order, OrderStatus},
    types::{UnifiedError, UnifiedResult},
};

#[derive(Debug, Clone)]
pub enum GTCEvent {
    OrderFilled {
        order_id: String,
        filled_quantity: f64,
    },
    OrderPartiallyFilled {
        order_id: String,
        filled_quantity: f64,
        remaining_quantity: f64,
    },
    OrderCancelled {
        order_id: String,
    },
    OrderExpired {
        order_id: String,
    },
}

#[derive(Debug)]
pub struct GTCOrderManager {
    active_orders: Arc<Mutex<HashMap<String, Order>>>,
    rx: Mutex<Receiver<GTCEvent>>,
    pub tx: Sender<GTCEvent>,
}

impl GTCOrderManager {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            active_orders: Arc::new(Mutex::new(HashMap::new())),
            rx: Mutex::new(rx),
            tx,
        }
    }

    pub fn get_event_sender(&self) -> Sender<GTCEvent> {
        self.tx.clone()
    }

    pub async fn add_gtc_order(&self, order: Order) -> UnifiedResult<String> {
        let order_id = order.id.clone();
        Self::new_persistence(&order)?;
        {
            let mut active_orders = self.active_orders.lock();
            active_orders.insert(order_id.clone(), order);
        }
        Ok("GTC order added successfully".to_string())
    }

    pub fn update_order_status(
        &self,
        order_id: &str,
        new_status: OrderStatus,
        filled_quantity: f64,
    ) -> UnifiedResult<String> {
        Self::update_persistence(order_id, new_status, filled_quantity)?;
        {
            let mut active_orders = self.active_orders.lock();
            match new_status {
                OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Expired => {
                    active_orders.remove(order_id);
                }
                OrderStatus::Partial => {
                    if let Some(order) = active_orders.get_mut(order_id) {
                        order.filled = filled_quantity;
                        order.status = OrderStatus::Partial;
                    }
                }
                _ => {}
            }
        }
        Ok("GTC order updated successfully".to_string())
    }

    pub async fn process_events(&self) {
        let receiver = self.rx.lock();
        while let Ok(event) = receiver.recv() {
            match event {
                GTCEvent::OrderFilled {
                    order_id,
                    filled_quantity,
                } => {
                    if let Err(e) =
                        self.update_order_status(&order_id, OrderStatus::Filled, filled_quantity)
                    {
                    }
                }
                GTCEvent::OrderPartiallyFilled {
                    order_id,
                    filled_quantity,
                    remaining_quantity: _,
                } => {
                    if let Err(e) =
                        self.update_order_status(&order_id, OrderStatus::Partial, filled_quantity)
                    {
                    }
                }
                GTCEvent::OrderCancelled { order_id } => {
                    if let Err(e) = self.update_order_status(&order_id, OrderStatus::Cancelled, 0.0)
                    {
                    }
                }
                GTCEvent::OrderExpired { order_id } => {
                    if let Err(e) = self.update_order_status(&order_id, OrderStatus::Expired, 0.0) {
                    }
                }
            }
        }
    }

    pub async fn start_event_loop(&self) {
        let manager = Arc::new(self.clone());
        std::thread::spawn(move || {
            manager.process_events();
        });
    }

    pub fn load_pending_gtc_orders(&self) -> UnifiedResult<Vec<Order>> {
        let orders = Self::load_persistence()?;
        {
            let mut active_orders = self.active_orders.lock();
            for order in &orders {
                active_orders.insert(order.id.clone(), order.clone());
            }
        }
        Ok(orders)
    }

    pub fn get_active_orders(&self) -> Vec<Order> {
        let active_orders = self.active_orders.lock();
        active_orders.values().cloned().collect()
    }

    pub fn get_order(&self, order_id: &str) -> Option<Order> {
        let active_orders = self.active_orders.lock();
        active_orders.get(order_id).cloned()
    }

    pub fn contains_order(&self, order_id: &str) -> bool {
        let active_orders = self.active_orders.lock();
        active_orders.contains_key(order_id)
    }

    pub fn active_order_count(&self) -> usize {
        let active_orders = self.active_orders.lock();
        active_orders.len()
    }

    // ----------- The persistence processing layer is not implemented yet. -----------

    fn new_persistence(_order: &Order) -> UnifiedResult<String> {
        Ok("ok".to_string())
    }

    fn update_persistence(
        _order_id: &str,
        _status: OrderStatus,
        _filled_quantity: f64,
    ) -> UnifiedResult<String> {
        Ok("ok".to_string())
    }

    fn load_persistence() -> UnifiedResult<Vec<Order>> {
        Ok(Vec::new())
    }
}

impl Clone for GTCOrderManager {
    fn clone(&self) -> Self {
        Self {
            active_orders: self.active_orders.clone(),
            rx: Mutex::new(self.rx.lock().clone()),
            tx: self.tx.clone(),
        }
    }
}
