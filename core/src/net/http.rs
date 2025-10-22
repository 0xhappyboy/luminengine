use std::{
    sync::{Arc, RwLock},
    time::Instant,
};

use axum::{
    Json, Router,
    extract::Query,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    config::HTTP_LISTENER_PORT, order::{Order, OrderDirection, OrderStatus}, orderbook::{OrderBook, OrderBooks}, target::Target
};

/// order book http service, HTTP service for handling order books
pub struct OrderBookHttpService;
impl OrderBookHttpService {
    pub async fn enable() {
        // build our application with a route
        let app = Router::new()
            .route(
                "/orderbook/create",
                post({ move |body| Self::create_order_book(body) }),
            )
            .route("/buy", post({ move |body| Self::push_order(body) }))
            .route("/sell", post({ move |body| Self::push_order(body) }))
            .route("/cancel", post({ move |body| Self::cancel(body) }))
            .route(
                "/getOrderNumByPrice",
                get(move |path| Self::get_order_num_by_price(path)),
            );
        let addr = HTTP_LISTENER_PORT.lock().unwrap().clone();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }
    // web api, get order num by price.
    async fn get_order_num_by_price(Query(vo): Query<OrderVO>) -> impl IntoResponse {
        let mut num = 0;
        if OrderBooks::contains_symbol(vo.clone().symbol) {
            let orderbook = OrderBooks::get_orderbook_by_symbol(vo.clone().symbol);
            match vo.direction {
                OrderDirection::Buy => {}
                OrderDirection::Sell => {}
                OrderDirection::None => (),
            }
        }
        (
            StatusCode::EXPECTATION_FAILED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "ok","data":0})),
        )
    }
    /// create order book
    async fn create_order_book(Json(vo): Json<CreateOrderBookVO>) -> impl IntoResponse {
        let orderbook = vo.to_orderbook();
        let orderbook = Arc::new(RwLock::new(orderbook));
        match OrderBooks::insert(vo.clone().symbol, orderbook) {
            Ok(s) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": s.to_string()})),
            ),
            Err(e) => (
                StatusCode::EXPECTATION_FAILED,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": e.to_string()})),
            ),
        }
    }
    /// http buy interface processing
    async fn push_order(Json(vo): Json<OrderVO>) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.clone().symbol) {
            let mut orderbook = OrderBooks::get_orderbook_by_symbol(vo.clone().symbol);
            let mut orderbook = orderbook.as_mut().unwrap().write().unwrap();
            let order = vo.to_order();
            orderbook.push_order(order);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "create successfully"})),
            )
        } else {
            (
                StatusCode::EXPECTATION_FAILED,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "symbol not exists"})),
            )
        }
    }
    /// http cancel order interface processing
    async fn cancel(Json(vo): Json<OrderVO>) -> impl IntoResponse {
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
        OrderBook::new(Target {
            symbol: self.symbol.clone(),
        })
    }
}

// order view object
#[derive(Deserialize, Serialize, Debug, Clone)]
struct OrderVO {
    pub id: String,
    pub symbol: String,
    pub price: f64,
    pub direction: OrderDirection,
    pub quantity: f64,
    pub remaining: f64,
    pub filled: f64,
    pub crt_time: String,
    pub status: OrderStatus,
    pub ex: Option<String>,
}

impl OrderVO {
    pub fn to_order(&self) -> Order {
        let order = Order {
            id: "id".to_string(),
            symbol: self.symbol.clone(),
            price: self.price,
            direction: self.direction.clone(),
            quantity: self.quantity,
            remaining: self.remaining,
            filled: self.filled,
            crt_time: Utc::now().to_string(),
            status: self.status.clone(),
            expiry: Some(Instant::now()),
            ex: None,
        };
        order
    }
}
