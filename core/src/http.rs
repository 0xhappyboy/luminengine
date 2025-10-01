use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use axum::{
    Json, Router,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::orderbook::{Order, OrderBook, OrderDirection, OrderTree};

const HTTP_LISTENER_PORT: &str = "0.0.0.0:8080";

/// order book http service, HTTP service for handling order books
pub struct OrderBookHttpService {}

impl OrderBookHttpService {
    pub async fn enable(
        orderbooks: Arc<RwLock<HashMap<String, OrderBook>>>,
        // bids: Arc<RwLock<OrderTree>>, asks: Arc<RwLock<OrderTree>>
    ) {
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
                    move |body| Self::buy(body, orderbooks)
                }),
            )
            .route(
                "/sell",
                post({
                    let orderbooks = Arc::clone(&orderbooks);
                    move |body| Self::sell(body, orderbooks)
                }),
            )
            .route(
                "/cancel",
                post({
                    let orderbooks = Arc::clone(&orderbooks);
                    move |body| Self::cancel(body, orderbooks)
                }),
            );
        // run our app with hyper, listening globally on port 3000
        let listener = tokio::net::TcpListener::bind(HTTP_LISTENER_PORT)
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
    }
    /// create order book
    async fn create_order_book(
        Json(vo): Json<CreateOrderBookVO>,
        orderbooks: Arc<RwLock<HashMap<String, OrderBook>>>,
    ) -> impl IntoResponse {
        let mut orderbooks = orderbooks.write().unwrap();
        orderbooks.insert(vo.clone().symbol, vo.to_orderbook());
        (
            StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(
                json!({ "message": "order placed successfully","request params":orderbooks.len(),"order book value":orderbooks.len().to_string()}),
            ),
        )
    }
    /// http buy interface processing
    async fn buy(
        Json(vo): Json<OrderVO>,
        orderbooks: Arc<RwLock<HashMap<String, OrderBook>>>,
    ) -> impl IntoResponse {
        (
            StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "order placed successfully","request params":vo.to_order() })),
        )
    }
    /// http sell interface processing
    async fn sell(
        Json(vo): Json<OrderVO>,
        orderbooks: Arc<RwLock<HashMap<String, OrderBook>>>,
    ) -> impl IntoResponse {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "order placed successfully","request params":vo.to_order() })),
        )
    }
    /// http cancel order interface processing
    async fn cancel(
        Json(vo): Json<OrderVO>,
        orderbooks: Arc<RwLock<HashMap<String, OrderBook>>>,
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
            self.symbol.clone(),
            self.price,
            self.order_direction.clone(),
        );
        order
    }
}
