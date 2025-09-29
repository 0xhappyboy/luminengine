use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::{self},
    time::Duration,
};

use crate::types::PushOrderEvent;

use axum::{
    Json, Router,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{join, task::JoinHandle};

const HTTP_LISTENER_PORT: &str = "0.0.0.0:8080";

#[derive(Deserialize, Serialize, Debug, Clone)]
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

/// order book RPC service, RPC service for handling order books
pub struct OrderBookRPCService {}
impl OrderBookRPCService {
    pub async fn enable() {
        todo!()
    }
}

/// order book TCP service, TCP service for handling order books
pub struct OrderBOOkTCPService {}
impl OrderBOOkTCPService {
    pub async fn enable() {
        todo!()
    }
}

/// order book http service, HTTP service for handling order books
struct OrderBookHttpService {}

impl OrderBookHttpService {
    pub async fn enable(buy_orders: Arc<Mutex<OrderTree>>, sell_orders: Arc<Mutex<OrderTree>>) {
        // build our application with a route
        let app = Router::new()
            .route(
                "/buy",
                post({
                    let buy_order = Arc::clone(&buy_orders);
                    move |body| Self::buy(body, buy_order)
                }),
            )
            .route(
                "/sell",
                post({
                    let sell_orders = Arc::clone(&sell_orders);
                    move |body| Self::sell(body, sell_orders)
                }),
            )
            .route(
                "/cancel",
                post({
                    let buy_order = Arc::clone(&buy_orders);
                    let sell_orders = Arc::clone(&sell_orders);
                    move |body| Self::cancel(body, buy_order, sell_orders)
                }),
            );
        // run our app with hyper, listening globally on port 3000
        let listener = tokio::net::TcpListener::bind(HTTP_LISTENER_PORT)
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
    }
    /// http buy interface processing
    async fn buy(Json(order): Json<Order>, buy: Arc<Mutex<OrderTree>>) -> impl IntoResponse {
        let buy = buy.lock();
        buy.unwrap().push(order.clone());
        (
            StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "order placed successfully","request params":order.clone() })),
        )
    }
    /// http sell interface processing
    async fn sell(Json(order): Json<Order>, sell: Arc<Mutex<OrderTree>>) -> impl IntoResponse {
        let sell = sell.lock();
        sell.unwrap().push(order.clone());
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "order placed successfully","request params":order.clone() })),
        )
    }
    /// http cancel order interface processing
    async fn cancel(
        Json(order): Json<Order>,
        buy: Arc<Mutex<OrderTree>>,
        sell: Arc<Mutex<OrderTree>>,
    ) -> impl IntoResponse {
        format!("push order cancel order").to_string()
    }
}

/// order book background service, the service will continue to run until the process ends.
struct BGService {}
impl BGService {
    pub async fn enable(buy_orders: Arc<Mutex<OrderTree>>, sell_orders: Arc<Mutex<OrderTree>>) {
        let _buy_order = Arc::clone(&buy_orders);
        let _sell_orders = Arc::clone(&sell_orders);
        loop {}
    }
}

/// for each order abstract
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Order {
    price: f64,
    order_type: OrderType,
    ex: Option<String>,
}

impl Order {
    pub fn new(price: f64, order_type: OrderType) -> Self {
        Self {
            price: price,
            order_type: order_type,
            ex: None,
        }
    }
}

/// order tree
#[derive(Debug, Clone)]
struct OrderTree {
    tree: HashMap<String, Vec<Order>>,
    // push buy order before event
    pub push_buy_order_before_event: Option<PushOrderEvent>,
    // push buy order after event
    pub push_buy_order_after_event: Option<PushOrderEvent>,
    // push sell order before event
    pub push_sell_order_before_event: Option<PushOrderEvent>,
    // push sell order after event
    pub push_sell_order_after_event: Option<PushOrderEvent>,
}

impl OrderTree {
    pub fn new(
        push_buy_order_before_event: Option<PushOrderEvent>,
        push_buy_order_after_event: Option<PushOrderEvent>,
        push_sell_order_before_event: Option<PushOrderEvent>,
        push_sell_order_after_event: Option<PushOrderEvent>,
    ) -> Self {
        Self {
            tree: HashMap::<String, Vec<Order>>::default(),
            push_buy_order_before_event: push_buy_order_before_event,
            push_buy_order_after_event: push_buy_order_after_event,
            push_sell_order_before_event: push_sell_order_before_event,
            push_sell_order_after_event: push_sell_order_after_event,
        }
    }
    // Determine whether the specified price exists in the order tree
    pub fn contains_price(&self, price: f64) -> bool {
        self.tree.contains_key(&price.to_string())
    }
    // Is the order tree empty?
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }
    // Push orders to the order tree
    pub fn push(&mut self, order: Order) {
        // before successfully placing the order
        match order.order_type {
            OrderType::Buy => {
                // buy order
                if self.push_buy_order_before_event.is_some() {
                    self.push_buy_order_before_event.unwrap()(order.clone());
                }
            }
            OrderType::Sell => {
                // sell order
                if self.push_sell_order_before_event.is_some() {
                    self.push_sell_order_before_event.unwrap()(order.clone());
                }
            }
        }
        if self.contains_price(order.clone().price) {
            self.tree
                .get_mut(&order.clone().price.to_string())
                .unwrap()
                .push(order.clone());
        } else {
            self.tree
                .insert(order.clone().price.to_string(), vec![order.clone()]);
        }
        // after successfully placing the order
        match order.order_type {
            OrderType::Buy => {
                if self.push_buy_order_after_event.is_some() {
                    self.push_buy_order_after_event.unwrap()(order);
                }
            }
            OrderType::Sell => {
                if self.push_sell_order_after_event.is_some() {
                    self.push_sell_order_after_event.unwrap()(order);
                }
            }
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

/// border book
#[derive(Debug, Clone)]
pub struct OrderBook {
    buy_orders: Arc<Mutex<OrderTree>>,
    sell_orders: Arc<Mutex<OrderTree>>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            buy_orders: Arc::new(Mutex::new(OrderTree::new(None, None, None, None))),
            sell_orders: Arc::new(Mutex::new(OrderTree::new(None, None, None, None))),
        }
    }
    /// start up order book
    pub async fn startup(&self, orderchannel: Vec<OrderChannel>) {
        let mut tasks: Vec<JoinHandle<()>> = Vec::new();
        let (buy_order, sell_order) = self.get_order_copy();
        // add order book backend service
        tasks.push(tokio::spawn(async move {
            BGService::enable(buy_order.clone(), sell_order).await;
        }));
        // added network channel processing tasks related to network order placement.
        orderchannel.iter().for_each(|c| match c {
            OrderChannel::Http => {
                // handle orders placed via the http protocol.
                let (buy_order, sell_order) = self.get_order_copy();
                tasks.push(tokio::spawn(async move {
                    OrderBookHttpService::enable(buy_order.clone(), sell_order).await;
                }));
            }
            OrderChannel::Tcp => {
                // handle orders placed via the tcp protocol.
                todo!()
            }
            OrderChannel::Rcp => {
                // handle orders placed via the rcp.
                todo!()
            }
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
        let buy_orders = buy_orders.lock().unwrap();
        let sell_orders = sell_orders.lock().unwrap();
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
    /// update push buy order before event
    pub fn update_push_buy_order_before_event(
        &mut self,
        push_buy_order_before_event: Option<PushOrderEvent>,
    ) -> Self {
        let mut buy_orders = self.buy_orders.lock().unwrap();
        buy_orders.push_buy_order_before_event = push_buy_order_before_event;
        self.clone()
    }
    /// update push buy order after event
    pub fn update_push_buy_order_after_event(
        &mut self,
        push_buy_order_after_event: Option<PushOrderEvent>,
    ) -> Self {
        let mut buy_orders = self.buy_orders.lock().unwrap();
        buy_orders.push_buy_order_after_event = push_buy_order_after_event;
        self.clone()
    }
    /// update push sell order before event
    pub fn update_push_sell_order_before_event(
        &mut self,
        push_sell_order_before_event: Option<PushOrderEvent>,
    ) -> Self {
        let mut sell_orders = self.sell_orders.lock().unwrap();
        sell_orders.push_sell_order_before_event = push_sell_order_before_event;
        self.clone()
    }
    /// update push sell order after event
    pub fn update_push_sell_order_after_event(
        &mut self,
        push_sell_order_after_event: Option<PushOrderEvent>,
    ) -> Self {
        let mut sell_orders = self.sell_orders.lock().unwrap();
        sell_orders.push_sell_order_after_event = push_sell_order_after_event;
        self.clone()
    }
}
