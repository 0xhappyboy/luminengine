use core::task;
use std::{
    collections::HashMap,
    f32::consts::E,
    sync::{Arc, Mutex},
    thread::{self},
    time::Duration,
};

use crate::sys::Sys;

use axum::{
    Extension, Router, body::Bytes, http::StatusCode, response::IntoResponse, routing::get,
};
use tokio::{join, task::JoinHandle};

const LISTENER_PORT: &str = "0.0.0.0:8080";

#[derive(Debug, Clone)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub enum OrderChannel {
    Http,
    Tcp,
    Rcp,
}

/// order book http service
pub struct OrderBookHttpService {}

impl OrderBookHttpService {
    pub async fn enable(buy_orders: Arc<Mutex<OrderTree>>, sell_orders: Arc<Mutex<OrderTree>>) {
        // build our application with a route
        let app = Router::new()
            .route("/buy", get(Self::buy))
            .layer(Extension(buy_orders))
            .route("/sell", get(Self::sell))
            .layer(Extension(sell_orders))
            .route("/cancel", get(Self::cancel));
        // run our app with hyper, listening globally on port 3000
        let listener = tokio::net::TcpListener::bind(LISTENER_PORT).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }
    async fn buy(Extension(buy): Extension<Arc<Mutex<OrderTree>>>) -> String {
        let mut buy = buy.lock();
        buy.unwrap().push(Order::new(11.1, OrderType::Buy));
        format!("buy {:?}", 123123).to_string()
    }
    async fn sell(Extension(sell): Extension<Arc<Mutex<OrderTree>>>) -> String {
        let mut sell = sell.lock();
        sell.unwrap().push(Order::new(11.1, OrderType::Buy));
        format!("sell {:?}", 123123).to_string()
    }
    async fn cancel() -> String {
        format!("push order cancel order").to_string()
    }
}

#[derive(Debug, Clone)]
pub struct Order {
    price: f64,
    order_type: OrderType,
    sys: Sys,
    ex: Option<String>,
}

impl Order {
    pub fn new(price: f64, order_type: OrderType) -> Self {
        Self {
            price: price,
            order_type: order_type,
            sys: Sys::new(),
            ex: None,
        }
    }
}

/// order tree
#[derive(Debug, Clone)]
pub struct OrderTree {
    tree: HashMap<String, Vec<Order>>,
}

impl OrderTree {
    pub fn new() -> Self {
        Self {
            tree: HashMap::<String, Vec<Order>>::default(),
        }
    }
    pub fn contains_price(&self, price: f64) -> bool {
        self.tree.contains_key(&price.to_string())
    }
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }
    pub fn push(&mut self, order: Order) {
        if self.contains_price(order.price) {
            self.tree
                .get_mut(&order.price.to_string())
                .unwrap()
                .push(order);
        } else {
            self.tree.insert(order.price.to_string(), vec![order]);
        }
    }
    /// specified price order quantity
    pub fn get_order_num_by_price(&self, price: f64) -> u64 {
        if self.tree.contains_key(&price.to_string()) {
            self.tree
                .get(&price.to_string())
                .unwrap()
                .len()
                .try_into()
                .unwrap()
        } else {
            0
        }
    }
    pub fn cancel(&mut self, order: Order) {}
}

#[derive(Debug, Clone)]
pub struct OrderBook {
    buy_orders: Arc<Mutex<OrderTree>>,
    sell_orders: Arc<Mutex<OrderTree>>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            buy_orders: Arc::new(Mutex::new(OrderTree::new())),
            sell_orders: Arc::new(Mutex::new(OrderTree::new())),
        }
    }
    /// start up order book
    pub async fn startup(&self, orderchannel: Vec<OrderChannel>) {
        let mut tasks: Vec<JoinHandle<()>> = Vec::new();
        orderchannel.iter().for_each(|c| match c {
            OrderChannel::Http => {
                let (buy_order, sell_order) = self.get_order_copy();
                tasks.push(tokio::spawn(async move {
                    OrderBookHttpService::enable(buy_order.clone(), sell_order).await;
                }));
            }
            OrderChannel::Tcp => {
                let a = self.clone();
                tasks.push(tokio::spawn(async move {
                    loop {
                        println!(
                            "buy order num {:?} sell order num {:?}",
                            a.order_num(Order::new(11.1, OrderType::Buy)).0,
                            a.order_num(Order::new(11.1, OrderType::Sell)).1
                        );
                        thread::sleep(Duration::from_millis(500));
                    }
                }));
            }
            OrderChannel::Rcp => todo!(),
        });
        for t in tasks {
            join!(t);
        }
    }
    /// local push order
    pub fn local_push_order(&self, order: Order) {
        let (mut a, mut b) = Self::get_order_copy(&self);
        tokio::spawn(async move {
            loop {
                thread::sleep(Duration::from_millis(2000));
                Self::push_order(&mut a, &mut b, Order::new(1111.1, OrderType::Buy));
            }
        });
    }
    /// tcp push order
    pub fn enable_tcp_push_order(&self) -> Self {
        let (mut a, mut b) = Self::get_order_copy(&self);
        tokio::spawn(async move {
            loop {
                thread::sleep(Duration::from_millis(2000));
                Self::push_order(&mut a, &mut b, Order::new(1111.1, OrderType::Buy));
            }
        });
        self.clone()
    }
    /// push order specific implementation logic.
    fn push_order(
        buy_orders: &mut Arc<Mutex<OrderTree>>,
        sell_orders: &mut Arc<Mutex<OrderTree>>,
        order: Order,
    ) {
        match order.order_type {
            OrderType::Buy => {
                let mut buy_orders = buy_orders.lock().unwrap();
                buy_orders.push(order);
                drop(buy_orders);
            }
            OrderType::Sell => {
                let mut sell_orders = sell_orders.lock().unwrap();
                sell_orders.push(order);
                drop(sell_orders);
            }
        }
    }
    /// matching order
    pub fn matching_order(&self) {}
    /// matching trading
    pub fn matching_trading(&self) {}
    /// get order num
    pub fn order_num(&self, order: Order) -> (u64, u64) {
        let (buy_orders, sell_orders) = self.get_order_copy();
        let mut buy_orders = buy_orders.lock().unwrap();
        let mut sell_orders = sell_orders.lock().unwrap();
        let buy_num = if buy_orders.is_empty() {
            0
        } else {
            buy_orders.get_order_num_by_price(order.price)
        };
        let sell_num = if sell_orders.is_empty() {
            0
        } else {
            sell_orders.get_order_num_by_price(order.price)
        };
        (buy_num.try_into().unwrap(), sell_num.try_into().unwrap())
    }
    /// order book storage
    pub fn storage() {}
    /// get order copy
    fn get_order_copy(&self) -> (Arc<Mutex<OrderTree>>, Arc<Mutex<OrderTree>>) {
        (Arc::clone(&self.buy_orders), Arc::clone(&self.sell_orders))
    }
}
