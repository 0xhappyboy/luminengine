use std::{
    collections::HashMap,
    panic::AssertUnwindSafe,
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
    orderbook::OrderBooks,
    orderbook::{Order, OrderBook, OrderDirection},
};

const HTTP_LISTENER_PORT: &str = "0.0.0.0:8080";

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
        // run our app with hyper, listening globally on port 3000
        let listener = tokio::net::TcpListener::bind(HTTP_LISTENER_PORT)
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
    }
    // web api, get order num by price.
    async fn get_order_num_by_price(Query(vo): Query<OrderVO>) -> impl IntoResponse {
        let mut num = 0;
        if OrderBooks::contains_symbol(vo.clone().symbol) {
            let orderbook = OrderBooks::get_orderbook_by_symbol(vo.clone().symbol);
            match vo.order_direction {
                OrderDirection::Buy => {
                    num = orderbook
                        .unwrap()
                        .read()
                        .unwrap()
                        .bids
                        .read()
                        .unwrap()
                        .get_order_num_by_price(vo.price);
                }
                OrderDirection::Sell => {
                    num = orderbook
                        .unwrap()
                        .read()
                        .unwrap()
                        .asks
                        .read()
                        .unwrap()
                        .get_order_num_by_price(vo.price);
                }
                OrderDirection::None => (),
            }
        }
        (
            StatusCode::EXPECTATION_FAILED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "ok","data":num})),
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
