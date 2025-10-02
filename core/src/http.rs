use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use axum::{
    Json, Router,
    extract::Query,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    matcher::Matcher,
    orderbook::{Order, OrderBook, OrderDirection},
};

const HTTP_LISTENER_PORT: &str = "0.0.0.0:8080";

/// order book http service, HTTP service for handling order books
pub struct OrderBookHttpService;
impl OrderBookHttpService {
    pub async fn enable(orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>) {
        // build our application with a route
        let app = Router::new()
            .route(
                "/orderbook/create",
                post({
                    let orderbooks = Arc::clone(&orderbooks);
                    move |body| Self::create_order_book(body, orderbooks)
                }),
            )
            .route(
                "/buy",
                post({
                    let orderbooks = Arc::clone(&orderbooks);
                    move |body| Self::push_order(body, orderbooks)
                }),
            )
            .route(
                "/sell",
                post({
                    let orderbooks = Arc::clone(&orderbooks);
                    move |body| Self::push_order(body, orderbooks)
                }),
            )
            .route(
                "/cancel",
                post({
                    let orderbooks = Arc::clone(&orderbooks);
                    move |body| Self::cancel(body, orderbooks)
                }),
            )
            .route(
                "/getOrderNumByPrice",
                get(move |path| Self::get_order_num_by_price(path, orderbooks)),
            );
        // run our app with hyper, listening globally on port 3000
        let listener = tokio::net::TcpListener::bind(HTTP_LISTENER_PORT)
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
    }
    // web api, get order num by price.
    async fn get_order_num_by_price(
        Query(vo): Query<OrderVO>,
        orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>,
    ) -> impl IntoResponse {
        let orderbooks = orderbooks.read().unwrap();
        let mut num = 0;
        if orderbooks.contains_key(&vo.clone().symbol) {
            let orderbook = orderbooks.get(&vo.clone().symbol).unwrap();
            match vo.order_direction {
                OrderDirection::Buy => {
                    num = orderbook
                        .read()
                        .unwrap()
                        .bids
                        .read()
                        .unwrap()
                        .get_order_num_by_price(vo.price);
                }
                OrderDirection::Sell => {
                    num = orderbook
                        .read()
                        .unwrap()
                        .asks
                        .read()
                        .unwrap()
                        .get_order_num_by_price(vo.price);
                }
            }
        }
        (
            StatusCode::EXPECTATION_FAILED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "ok","data":num})),
        )
    }
    /// create order book
    async fn create_order_book(
        Json(vo): Json<CreateOrderBookVO>,
        orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>,
    ) -> impl IntoResponse {
        let mut orderbooks = orderbooks.write().unwrap();
        if !orderbooks.contains_key(&vo.clone().symbol) {
            let orderbook = vo.to_orderbook();
            let orderbook = Arc::new(RwLock::new(orderbook));
            orderbooks.insert(vo.clone().symbol, orderbook);
            orderbooks
                .get(&vo.symbol.clone())
                .unwrap()
                .read()
                .unwrap()
                .enble_matcher(Matcher::new());
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "create successfully"})),
            )
        } else {
            (
                StatusCode::EXPECTATION_FAILED,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "symbol already exists"})),
            )
        }
    }
    /// http buy interface processing
    async fn push_order(
        Json(vo): Json<OrderVO>,
        orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>,
    ) -> impl IntoResponse {
        let mut orderbooks = orderbooks.write().unwrap();
        if orderbooks.contains_key(&vo.clone().symbol) {
            let orderbook = orderbooks.get_mut(&vo.clone().symbol).unwrap();
            let mut orderbook = orderbook.write().unwrap();
            let order = vo.to_order();
            orderbook.push_order(order);
            drop(orderbook);
            drop(orderbooks);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "create successfully"})),
            )
        } else {
            drop(orderbooks);
            (
                StatusCode::EXPECTATION_FAILED,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "symbol not exists"})),
            )
        }
    }
    /// http cancel order interface processing
    async fn cancel(
        Json(vo): Json<OrderVO>,
        orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>,
    ) -> impl IntoResponse {
        format!("push order cancel order").to_string()
    }
}

// create order book view object
#[derive(Deserialize, Serialize, Debug, Clone)]
struct CreateOrderBookVO {
    pub symbol: String,
}
impl CreateOrderBookVO {
    pub fn to_orderbook(&self) -> OrderBook {
        OrderBook::new(self.symbol.clone())
    }
}

// order view object
#[derive(Deserialize, Serialize, Debug, Clone)]
struct OrderVO {
    pub symbol: String,
    pub price: f64,
    pub order_direction: OrderDirection,
    pub ex: Option<String>,
}

impl OrderVO {
    pub fn to_order(&self) -> Order {
        let order = Order::new(
            "1".to_string(),
            self.symbol.clone(),
            self.price,
            self.order_direction.clone(),
        );
        order
    }
}
