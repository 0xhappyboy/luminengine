use std::sync::{Arc, RwLock};

use axum::{
    Json, Router,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use serde_json::json;

use crate::orderbook::{Order, OrderBook, OrderTree};

const HTTP_LISTENER_PORT: &str = "0.0.0.0:8080";

/// order book http service, HTTP service for handling order books
pub struct OrderBookHttpService {
    orderbook: Arc<RwLock<Vec<OrderBook>>>,
}

impl OrderBookHttpService {
    pub async fn enable(bids: Arc<RwLock<OrderTree>>, asks: Arc<RwLock<OrderTree>>) {
        // build our application with a route
        let app = Router::new()
            .route(
                "/buy",
                post({
                    let bids = Arc::clone(&bids);
                    move |body| Self::buy(body, bids)
                }),
            )
            .route(
                "/sell",
                post({
                    let asks = Arc::clone(&asks);
                    move |body| Self::sell(body, asks)
                }),
            )
            .route(
                "/cancel",
                post({
                    let bids = Arc::clone(&bids);
                    let asks = Arc::clone(&asks);
                    move |body| Self::cancel(body, bids, asks)
                }),
            );
        // run our app with hyper, listening globally on port 3000
        let listener = tokio::net::TcpListener::bind(HTTP_LISTENER_PORT)
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
    }
    /// http buy interface processing
    async fn buy(Json(order): Json<Order>, buy: Arc<RwLock<OrderTree>>) -> impl IntoResponse {
        let buy = buy.write();
        buy.unwrap().push(order.clone());
        (
            StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "order placed successfully","request params":order.clone() })),
        )
    }
    /// http sell interface processing
    async fn sell(Json(order): Json<Order>, sell: Arc<RwLock<OrderTree>>) -> impl IntoResponse {
        let sell = sell.write();
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
        buy: Arc<RwLock<OrderTree>>,
        sell: Arc<RwLock<OrderTree>>,
    ) -> impl IntoResponse {
        format!("push order cancel order").to_string()
    }
}
