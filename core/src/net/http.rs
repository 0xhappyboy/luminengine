use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{Path, Query},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    config::HTTP_LISTENER_PORT,
    order::{Order, OrderDirection, OrderStatus, OrderType},
    orderbook::{OrderBook, OrderBooks},
    price::PriceLevel,
    target::Target,
};

/// order book http service, HTTP service for handling order books
pub struct OrderBookHttpService;
impl OrderBookHttpService {
    pub async fn enable() {
        // build our application with a route
        let app = Router::new()
            .route("/orderbook/create", post(Self::create_order_book))
            .route("/orderbook/{symbol}/depth", get(Self::get_market_depth))
            .route("/orderbook/{symbol}/stats", get(Self::get_engine_stats))
            .route("/orderbook/{symbol}/config", get(Self::get_engine_config))
            .route(
                "/orderbook/{symbol}/config",
                post(Self::update_engine_config),
            )
            .route("/orderbook/{symbol}/info", get(Self::get_orderbook_info))
            .route("/orderbook/{symbol}/reset", post(Self::reset_orderbook))
            .route("/orderbooks", get(Self::list_all_orderbooks))
            .route("/orderbook/{symbol}/delete", post(Self::delete_orderbook))
            .route("/order/buy", post(Self::push_buy_order))
            .route("/order/sell", post(Self::push_sell_order))
            .route("/order/cancel", post(Self::cancel_order))
            .route("/order/batch-cancel", post(Self::batch_cancel_orders))
            .route("/order/{order_id}/location", get(Self::get_order_location))
            .route("/order/{order_id}/status", get(Self::get_order_status))
            .route("/orders/{symbol}/active", get(Self::get_active_orders))
            .route("/order/{order_id}/modify", post(Self::set_order))
            .route("/order/stop/buy", post(Self::add_stop_buy_order))
            .route("/order/stop/sell", post(Self::add_stop_sell_order))
            .route("/order/stop/cancel", post(Self::cancel_stop_order))
            .route(
                "/order/stop/{order_id}/status",
                get(Self::get_stop_order_status),
            )
            .route(
                "/order/stop/{order_id}/modify",
                post(Self::modify_stop_order),
            )
            .route("/order/gtc/{order_id}", get(Self::get_gtc_order))
            .route("/order/gtc/active", get(Self::get_active_gtc_orders))
            .route("/order/gtc/reload", post(Self::reload_gtc_orders))
            .route(
                "/market/{symbol}/price/current",
                get(Self::get_current_price),
            )
            .route(
                "/market/{symbol}/price/last",
                get(Self::get_last_match_price),
            )
            .route("/market/{symbol}/price/mid", get(Self::get_mid_price))
            .route("/market/{symbol}/spread", get(Self::get_spread))
            .route(
                "/market/{symbol}/order-count",
                get(Self::get_total_order_count),
            )
            .route(
                "/market/{symbol}/available-quantity",
                post(Self::get_available_quantity),
            )
            .route("/market/{symbol}/volume", get(Self::get_trading_volume))
            .route("/market/{symbol}/liquidity", get(Self::get_liquidity_info))
            .route("/market/{symbol}/best-prices", get(Self::get_best_prices))
            .route("/order/ioc/check", post(Self::check_ioc_feasibility))
            .route("/order/ioc/buy", post(Self::push_ioc_buy_order))
            .route("/order/ioc/sell", post(Self::push_ioc_sell_order))
            .route(
                "/engine/{symbol}/cleanup",
                post(Self::cleanup_expired_orders),
            )
            .route(
                "/engine/{symbol}/trigger-match",
                post(Self::trigger_immediate_match),
            )
            .route("/engine/{symbol}/start", post(Self::start_engine))
            .route("/engine/{symbol}/stop", post(Self::stop_engine))
            .route("/engine/{symbol}/health", get(Self::get_engine_health))
            .route("/engine/{symbol}/pause", post(Self::pause_engine))
            .route("/engine/{symbol}/resume", post(Self::resume_engine))
            .route("/batch/orders/create", post(Self::batch_create_orders))
            .route(
                "/batch/orders/cancel",
                post(Self::batch_cancel_orders_by_filter),
            )
            .route("/system/status", get(Self::get_system_status))
            .route("/system/metrics", get(Self::get_system_metrics));

        let addr = HTTP_LISTENER_PORT.lock().unwrap().clone();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    /// Get market depth snapshot
    pub async fn get_market_depth(
        Path(symbol): Path<String>,
        Query(params): Query<DepthParams>,
    ) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let snapshot_data = orderbook.get_market_depth_snapshot(params.levels);
            let vo = MarketDepthSnapshotVO::from_snapshot(snapshot_data);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": true,
                    "symbol": symbol,
                    "data": vo
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// Get engine statistics
    async fn get_engine_stats(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let status = orderbook.get_engine_status();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "message": "success",
                    "data": SlfeStatusVO::from_status(&status.read())
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get engine configuration
    async fn get_engine_config(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let config = orderbook.get_engine_config();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "message": "success",
                    "data": config
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Update engine configuration
    async fn update_engine_config(
        Path(symbol): Path<String>,
        Json(config): Json<crate::matchengine::MatchEngineConfig>,
    ) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            orderbook.update_engine_config(config);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Configuration updated successfully" })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Push buy order
    async fn push_buy_order(Json(vo): Json<OrderVO>) -> impl IntoResponse {
        Self::push_order_internal(vo, OrderDirection::Buy).await
    }

    /// Push sell order
    async fn push_sell_order(Json(vo): Json<OrderVO>) -> impl IntoResponse {
        Self::push_order_internal(vo, OrderDirection::Sell).await
    }

    /// Internal order push implementation
    async fn push_order_internal(vo: OrderVO, direction: OrderDirection) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                let mut order = vo.to_order();
                order.direction = direction;
                match orderbook.add_order(order) {
                    Ok(result) => (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": "Order created successfully", "data": result })),
                    ),
                    Err(e) => (
                        StatusCode::BAD_REQUEST,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": format!("Failed to create order: {:?}", e) })),
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Cancel order
    async fn cancel_order(Json(vo): Json<CancelOrderVO>) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                orderbook.cancel_order(&vo.order_id);
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Order cancelled successfully" })),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Batch cancel orders
    async fn batch_cancel_orders(Json(vo): Json<BatchCancelVO>) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                let order_ids: Vec<&str> = vo.order_ids.iter().map(|s| s.as_str()).collect();
                orderbook.batch_cancel_orders(&order_ids).await;
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Orders cancelled successfully" })),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get orderbook information
    async fn get_orderbook_info(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let target = orderbook.get_target();
            let total_orders = orderbook.get_total_active_orders();
            let current_price = orderbook.get_current_price();
            let last_price = orderbook.get_last_match_price();
            let spread = orderbook.get_spread();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": true,
                    "data": {
                        "symbol": target.symbol,
                        "total_active_orders": total_orders,
                        "current_price": current_price,
                        "last_match_price": last_price,
                        "spread": spread,
                        "timestamp": Utc::now().to_rfc3339()
                    }
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// List all orderbooks
    async fn list_all_orderbooks() -> impl IntoResponse {
        let orderbook_count = OrderBooks::order_num();
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({
                "success": true,
                "data": {
                    "total_orderbooks": orderbook_count,
                    "timestamp": Utc::now().to_rfc3339()
                }
            })),
        )
    }

    /// set order
    async fn set_order(
        Path(order_id): Path<String>,
        Json(vo): Json<ModifyOrderVO>,
    ) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                orderbook.cancel_order(&order_id);
                let mut new_order = vo.to_order();
                new_order.id = order_id;
                match orderbook.add_order(new_order) {
                    Ok(result) => (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({
                            "message": "Order modified successfully",
                            "data": result
                        })),
                    ),
                    Err(e) => (
                        StatusCode::BAD_REQUEST,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({
                            "message": format!("Failed to modify order: {:?}", e)
                        })),
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get active orders for symbol
    async fn get_active_orders(
        Path(symbol): Path<String>,
        Query(params): Query<ActiveOrdersParams>,
    ) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let orders = orderbook.get_active_gtc_orders();
            let filtered_orders: Vec<OrderVO> = orders
                .into_iter()
                .filter(|order| {
                    params
                        .direction
                        .clone()
                        .map_or(true, |dir| order.direction == dir)
                })
                .map(|order| OrderVO::from_order(&order))
                .collect();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": true,
                    "data": {
                        "symbol": symbol,
                        "orders": filtered_orders,
                        "count": filtered_orders.len()
                    }
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// Get order status
    async fn get_order_status(
        Path(order_id): Path<String>,
        Query(params): Query<OrderStatusParams>,
    ) -> impl IntoResponse {
        if OrderBooks::contains_symbol(params.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(params.symbol.clone()) {
                if let Some(order) = orderbook.get_gtc_order(&order_id) {
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({
                            "success": true,
                            "data": OrderVO::from_order(&order)
                        })),
                    )
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({
                            "success": false,
                            "message": "Order not found"
                        })),
                    )
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Modify stop order
    async fn modify_stop_order(
        Path(order_id): Path<String>,
        Json(vo): Json<ModifyStopOrderVO>,
    ) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                match orderbook.modify_stop_order(&order_id, vo.new_stop_price, vo.new_limit_price)
                {
                    Ok(success) => {
                        if success {
                            (
                                StatusCode::OK,
                                [(header::CONTENT_TYPE, "application/json")],
                                Json(json!({ "message": "Stop order modified successfully" })),
                            )
                        } else {
                            (
                                StatusCode::NOT_FOUND,
                                [(header::CONTENT_TYPE, "application/json")],
                                Json(json!({ "message": "Stop order not found" })),
                            )
                        }
                    }
                    Err(e) => (
                        StatusCode::BAD_REQUEST,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": format!("Failed to modify stop order: {:?}", e) })),
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get trading volume
    async fn get_trading_volume(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let status = orderbook.get_engine_status();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": true,
                    "data": {
                        "symbol": symbol,
                        "total_volume": status.read().total_quantity,
                        "orders_processed": status.read().orders_processed,
                        "orders_matched": status.read().orders_matched,
                        "timestamp": Utc::now().to_rfc3339()
                    }
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// Get liquidity information
    async fn get_liquidity_info(
        Path(symbol): Path<String>,
        Query(params): Query<LiquidityParams>,
    ) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let depth = orderbook.get_market_depth_snapshot(params.levels);
            let total_bid_liquidity: f64 = depth.bids.iter().map(|level| level.quantity).sum();
            let total_ask_liquidity: f64 = depth.asks.iter().map(|level| level.quantity).sum();

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": true,
                    "data": {
                        "symbol": symbol,
                        "total_bid_liquidity": total_bid_liquidity,
                        "total_ask_liquidity": total_ask_liquidity,
                        "liquidity_ratio": total_bid_liquidity / total_ask_liquidity.max(1.0),
                        "bid_levels": depth.bids.len(),
                        "ask_levels": depth.asks.len(),
                        "timestamp": Utc::now().to_rfc3339()
                    }
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// Get best bid/ask prices
    async fn get_best_prices(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let depth = orderbook.get_market_depth_snapshot(Some(1));
            let best_bid = depth.bids.first().map(|level| level.price).unwrap_or(0.0);
            let best_ask = depth.asks.first().map(|level| level.price).unwrap_or(0.0);

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": true,
                    "data": {
                        "symbol": symbol,
                        "best_bid": best_bid,
                        "best_ask": best_ask,
                        "spread": (best_ask - best_bid).abs(),
                        "timestamp": Utc::now().to_rfc3339()
                    }
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// Push IOC buy order
    async fn push_ioc_buy_order(Json(vo): Json<OrderVO>) -> impl IntoResponse {
        Self::push_ioc_order_internal(vo, OrderDirection::Buy).await
    }

    /// Push IOC sell order
    async fn push_ioc_sell_order(Json(vo): Json<OrderVO>) -> impl IntoResponse {
        Self::push_ioc_order_internal(vo, OrderDirection::Sell).await
    }

    /// Internal IOC order implementation
    async fn push_ioc_order_internal(vo: OrderVO, direction: OrderDirection) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                let mut order = vo.to_order();
                order.direction = direction;
                order.order_type = OrderType::IOC;

                match orderbook.add_order(order) {
                    Ok(result) => (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(
                            json!({ "message": "IOC order created successfully", "data": result }),
                        ),
                    ),
                    Err(e) => (
                        StatusCode::BAD_REQUEST,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": format!("Failed to create IOC order: {:?}", e) })),
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get engine health status
    async fn get_engine_health(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let status = orderbook.get_engine_status();
            let is_healthy = status.read().current_queue_depth < 1000; // 简单的健康检查逻辑
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": true,
                    "data": {
                        "symbol": symbol,
                        "healthy": is_healthy,
                        "queue_depth": status.read().current_queue_depth,
                        "match_rate": status.read().orders_matched as f64 / status.read().orders_processed as f64,
                        "avg_latency_us": status.read().avg_match_latency_us,
                        "timestamp": Utc::now().to_rfc3339()
                    }
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// Stop engine
    async fn stop_engine(Path(_symbol): Path<String>) -> impl IntoResponse {
        (
            StatusCode::NOT_IMPLEMENTED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "Stop engine not implemented" })),
        )
    }

    /// Pause engine
    async fn pause_engine(Path(_symbol): Path<String>) -> impl IntoResponse {
        (
            StatusCode::NOT_IMPLEMENTED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "Pause engine not implemented" })),
        )
    }

    /// Resume engine
    async fn resume_engine(Path(_symbol): Path<String>) -> impl IntoResponse {
        (
            StatusCode::NOT_IMPLEMENTED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "Resume engine not implemented" })),
        )
    }

    /// Reset orderbook
    async fn reset_orderbook(Path(_symbol): Path<String>) -> impl IntoResponse {
        (
            StatusCode::NOT_IMPLEMENTED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "Reset orderbook not implemented" })),
        )
    }

    /// Delete orderbook
    async fn delete_orderbook(Path(_symbol): Path<String>) -> impl IntoResponse {
        (
            StatusCode::NOT_IMPLEMENTED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "Delete orderbook not implemented" })),
        )
    }

    /// Get system status
    async fn get_system_status() -> impl IntoResponse {
        let orderbook_count = OrderBooks::order_num();

        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({
                "success": true,
                "data": {
                    "total_orderbooks": orderbook_count,
                    "timestamp": Utc::now().to_rfc3339(),
                    "status": "running"
                }
            })),
        )
    }

    /// Get system metrics
    async fn get_system_metrics() -> impl IntoResponse {
        let orderbook_count = OrderBooks::order_num();

        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({
                "success": true,
                "data": {
                    "orderbooks": orderbook_count,
                    "uptime": "0",
                    "memory_usage": "0",
                    "timestamp": Utc::now().to_rfc3339()
                }
            })),
        )
    }

    /// Batch create orders
    async fn batch_create_orders(Json(vo): Json<BatchCreateOrdersVO>) -> impl IntoResponse {
        let mut results = Vec::new();
        for order_vo in vo.orders {
            results.push(BatchOrderResult {
                order_id: order_vo.id.clone(),
                success: true,
                message: "Processed".to_string(),
            });
        }
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({
                "success": true,
                "data": {
                    "processed": results.len(),
                    "results": results
                }
            })),
        )
    }

    /// Batch cancel orders by filter
    async fn batch_cancel_orders_by_filter(
        Json(_vo): Json<BatchCancelFilterVO>,
    ) -> impl IntoResponse {
        (
            StatusCode::NOT_IMPLEMENTED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({ "message": "Batch cancel by filter not implemented" })),
        )
    }

    /// Get order location
    async fn get_order_location(
        Path(order_id): Path<String>,
        Query(params): Query<OrderLocationParams>,
    ) -> impl IntoResponse {
        if OrderBooks::contains_symbol(params.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(params.symbol.clone()) {
                if let Some(location) = orderbook.get_order_location(&order_id) {
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({
                            "message": "success",
                            "data": location
                        })),
                    )
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": "Order not found" })),
                    )
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Add stop buy order
    async fn add_stop_buy_order(Json(vo): Json<StopOrderVO>) -> impl IntoResponse {
        Self::add_stop_order_internal(vo, OrderDirection::Buy).await
    }

    /// Add stop sell order
    async fn add_stop_sell_order(Json(vo): Json<StopOrderVO>) -> impl IntoResponse {
        Self::add_stop_order_internal(vo, OrderDirection::Sell).await
    }

    /// Internal stop order implementation
    async fn add_stop_order_internal(
        vo: StopOrderVO,
        direction: OrderDirection,
    ) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                let mut order = vo.to_order();
                order.direction = direction;
                match orderbook.add_stop_order(order, vo.stop_price, vo.expiry_seconds) {
                    Ok(result) => (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(
                            json!({ "message": "Stop order created successfully", "data": result }),
                        ),
                    ),
                    Err(e) => (
                        StatusCode::BAD_REQUEST,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": format!("Failed to create stop order: {:?}", e) })),
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Cancel stop order
    async fn cancel_stop_order(Json(vo): Json<CancelStopOrderVO>) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                match orderbook.cancel_stop_order(&vo.order_id) {
                    Ok(success) => {
                        if success {
                            (
                                StatusCode::OK,
                                [(header::CONTENT_TYPE, "application/json")],
                                Json(json!({ "message": "Stop order cancelled successfully" })),
                            )
                        } else {
                            (
                                StatusCode::NOT_FOUND,
                                [(header::CONTENT_TYPE, "application/json")],
                                Json(json!({ "message": "Stop order not found" })),
                            )
                        }
                    }
                    Err(e) => (
                        StatusCode::BAD_REQUEST,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": format!("Failed to cancel stop order: {:?}", e) })),
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get stop order status
    async fn get_stop_order_status(
        Path(order_id): Path<String>,
        Query(params): Query<OrderStatusParams>,
    ) -> impl IntoResponse {
        if OrderBooks::contains_symbol(params.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(params.symbol.clone()) {
                if let Some(status) = orderbook.get_stop_order_status(&order_id) {
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({
                            "message": "success",
                            "data": status
                        })),
                    )
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": "Stop order not found" })),
                    )
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get GTC order
    async fn get_gtc_order(
        Path(order_id): Path<String>,
        Query(params): Query<OrderStatusParams>,
    ) -> impl IntoResponse {
        if OrderBooks::contains_symbol(params.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(params.symbol.clone()) {
                if let Some(order) = orderbook.get_gtc_order(&order_id) {
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({
                            "message": "success",
                            "data": OrderVO::from_order(&order)
                        })),
                    )
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": "GTC order not found" })),
                    )
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get active GTC orders
    async fn get_active_gtc_orders(Query(params): Query<OrderStatusParams>) -> impl IntoResponse {
        if OrderBooks::contains_symbol(params.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(params.symbol.clone()) {
                let orders: Vec<OrderVO> = orderbook
                    .get_active_gtc_orders()
                    .iter_mut()
                    .map(|order| OrderVO::from_order(order))
                    .collect();
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({
                        "message": "success",
                        "data": orders,
                        "count": orders.len()
                    })),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Reload GTC orders
    async fn reload_gtc_orders(Query(params): Query<OrderStatusParams>) -> impl IntoResponse {
        if OrderBooks::contains_symbol(params.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(params.symbol.clone()) {
                match orderbook.reload_pending_gtc_orders() {
                    Ok(orders) => (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({
                            "message": "success",
                            "data": orders.iter().map(|order| OrderVO::from_order(order)).collect::<Vec<OrderVO>>(),
                            "count": orders.len()
                        })),
                    ),
                    Err(e) => (
                        StatusCode::BAD_REQUEST,
                        [(header::CONTENT_TYPE, "application/json")],
                        Json(json!({ "message": format!("Failed to reload GTC orders: {:?}", e) })),
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get current price
    async fn get_current_price(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let price = orderbook.get_current_price();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "message": "success",
                    "data": price
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get last match price
    async fn get_last_match_price(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let price = orderbook.get_last_match_price();

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "message": "success",
                    "data": price
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get mid price
    async fn get_mid_price(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let price = orderbook.get_mid_price();

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "message": "success",
                    "data": price
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get spread
    async fn get_spread(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let spread = orderbook.get_spread();

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "message": "success",
                    "data": spread
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get total order count
    async fn get_total_order_count(
        Path(symbol): Path<String>,
        Query(params): Query<DirectionParams>,
    ) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            let count = orderbook.get_total_order_count_by_direction(params.direction);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "message": "success",
                    "data": count
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Get available quantity for an order
    async fn get_available_quantity(Json(vo): Json<OrderVO>) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                let order = vo.to_order();
                let available = orderbook.engine.get_available_quantity_for_order(&order);

                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({
                        "message": "success",
                        "data": available
                    })),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Failed to get orderbook" })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Check IOC order feasibility
    pub async fn check_ioc_feasibility(Json(vo): Json<OrderVO>) -> impl IntoResponse {
        if OrderBooks::contains_symbol(vo.symbol.clone()) {
            if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(vo.symbol.clone()) {
                let order = vo.to_order();
                let (feasible, available) = orderbook.check_ioc_feasibility(&order);

                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({
                        "success": true,
                        "data": {
                            "feasible": feasible,
                            "available_quantity": available
                        }
                    })),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({
                        "success": false,
                        "message": "Failed to get orderbook"
                    })),
                )
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// Cleanup expired orders
    async fn cleanup_expired_orders(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            match orderbook.cleanup_expired_orders().await {
                Ok(count) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({
                        "success": true,
                        "message": "Expired orders cleaned up successfully",
                        "data": count
                    })),
                ),
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({
                        "success": false,
                        "message": format!("Failed to cleanup expired orders: {:?}", e)
                    })),
                ),
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({
                    "success": false,
                    "message": "Symbol not found"
                })),
            )
        }
    }

    /// Trigger immediate match
    async fn trigger_immediate_match(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            match orderbook.trigger_immediate_match() {
                Ok(result) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Match triggered successfully", "data": result })),
                ),
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": format!("Failed to trigger match: {:?}", e) })),
                ),
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// Start engine
    async fn start_engine(Path(symbol): Path<String>) -> impl IntoResponse {
        if let Some(orderbook) = OrderBooks::get_orderbook_by_symbol(symbol.clone()) {
            match orderbook.start_engine().await {
                Ok(result) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": "Engine started successfully", "data": result })),
                ),
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    [(header::CONTENT_TYPE, "application/json")],
                    Json(json!({ "message": format!("Failed to start engine: {:?}", e) })),
                ),
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({ "message": "Symbol not found" })),
            )
        }
    }

    /// create order book
    async fn create_order_book(Json(vo): Json<CreateOrderBookVO>) -> impl IntoResponse {
        let orderbook = vo.to_orderbook();
        let orderbook = Arc::new(orderbook);
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
}

// order view object
/// Order View Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderVO {
    pub id: String,
    pub symbol: String,
    pub price: f64,
    pub direction: OrderDirection,
    pub quantity: f64,
    pub remaining: f64,
    pub filled: f64,
    pub crt_time: String,
    pub status: OrderStatus,
    pub expiry: Option<String>,
    pub order_type: OrderType,
    pub ex: Option<String>,
    pub fill_rate: f64,
}

impl OrderVO {
    /// order to order vo
    pub fn from_order(order: &Order) -> Self {
        let fill_rate = if order.quantity > 0.0 {
            order.filled / order.quantity
        } else {
            0.0
        };
        let expiry = order.expiry.map(|instant| {
            let system_time = SystemTime::now() - instant.elapsed();
            chrono::DateTime::<chrono::Utc>::from(system_time).to_rfc3339()
        });
        Self {
            id: order.id.clone(),
            symbol: order.symbol.clone(),
            price: order.price,
            direction: order.direction.clone(),
            quantity: order.quantity,
            remaining: order.remaining,
            filled: order.filled,
            crt_time: order.crt_time.clone(),
            status: order.status.clone(),
            expiry,
            order_type: order.order_type.clone(),
            ex: order.ex.clone(),
            fill_rate,
        }
    }

    /// order vo to order
    pub fn to_order(&self) -> Order {
        let expiry = self.expiry.as_ref().and_then(|expiry_str| {
            chrono::DateTime::parse_from_rfc3339(expiry_str)
                .ok()
                .map(|datetime| {
                    let system_time: SystemTime = datetime.into();
                    let duration_since_epoch = system_time
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default();
                    let now_duration = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default();
                    if duration_since_epoch > now_duration {
                        let remaining = duration_since_epoch - now_duration;
                        Instant::now() + remaining
                    } else {
                        Instant::now()
                    }
                })
        });
        crate::order::Order {
            id: self.id.clone(),
            symbol: self.symbol.clone(),
            price: self.price,
            direction: self.direction.clone(),
            quantity: self.quantity,
            remaining: self.remaining,
            filled: self.filled,
            crt_time: self.crt_time.clone(),
            status: self.status.clone(),
            expiry,
            order_type: self.order_type.clone(),
            ex: self.ex.clone(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DepthParams {
    pub levels: Option<usize>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct OrderLocationParams {
    pub symbol: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct OrderStatusParams {
    pub symbol: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct DirectionParams {
    pub direction: Option<OrderDirection>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct CancelOrderVO {
    pub symbol: String,
    pub order_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct BatchCancelVO {
    pub symbol: String,
    pub order_ids: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct CancelStopOrderVO {
    pub symbol: String,
    pub order_id: String,
}

// 新增的 VO 结构
#[derive(Deserialize, Serialize, Debug, Clone)]
struct ActiveOrdersParams {
    pub direction: Option<OrderDirection>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct LiquidityParams {
    pub levels: Option<usize>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ModifyStopOrderVO {
    pub symbol: String,
    pub new_stop_price: f64,
    pub new_limit_price: Option<f64>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct BatchCreateOrdersVO {
    pub orders: Vec<OrderVO>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct BatchOrderResult {
    pub order_id: String,
    pub success: bool,
    pub message: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct BatchCancelFilterVO {
    pub symbol: String,
    pub direction: Option<OrderDirection>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct StopOrderVO {
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
    pub stop_price: f64,
    pub expiry_seconds: Option<u64>,
}

impl StopOrderVO {
    pub fn to_order(&self) -> Order {
        Order {
            id: self.id.clone(),
            symbol: self.symbol.clone(),
            price: self.price,
            direction: self.direction.clone(),
            quantity: self.quantity,
            remaining: self.remaining,
            filled: self.filled,
            crt_time: Utc::now().to_string(),
            status: self.status.clone(),
            expiry: Some(Instant::now()),
            ex: self.ex.clone(),
            order_type: OrderType::Limit,
        }
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

/// Market Depth Snapshot View Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDepthSnapshotVO {
    pub bids: Vec<PriceLevelVO>,
    pub asks: Vec<PriceLevelVO>,
    pub timestamp: String,
    pub total_bid_orders: usize,
    pub total_ask_orders: usize,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ModifyOrderVO {
    pub symbol: String,
    pub price: f64,
    pub quantity: f64,
    pub direction: OrderDirection,
    pub order_type: OrderType,
    pub expiry: Option<String>,
}

impl ModifyOrderVO {
    pub fn to_order(&self) -> Order {
        let expiry = self.expiry.as_ref().and_then(|expiry_str| {
            chrono::DateTime::parse_from_rfc3339(expiry_str)
                .ok()
                .map(|datetime| {
                    let system_time: SystemTime = datetime.into();
                    let duration_since_epoch = system_time
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default();
                    let now_duration = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default();
                    if duration_since_epoch > now_duration {
                        let remaining = duration_since_epoch - now_duration;
                        Instant::now() + remaining
                    } else {
                        Instant::now()
                    }
                })
        });

        Order {
            id: "".to_string(),
            symbol: self.symbol.clone(),
            price: self.price,
            direction: self.direction.clone(),
            quantity: self.quantity,
            remaining: self.quantity,
            filled: 0.0,
            crt_time: Utc::now().to_string(),
            status: OrderStatus::Pending,
            expiry,
            order_type: self.order_type.clone(),
            ex: None,
        }
    }
}

/// Price Level View Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevelVO {
    pub price: f64,
    pub quantity: f64,
    pub order_count: usize,
}

impl MarketDepthSnapshotVO {
    /// Create from original MarketDepthSnapshot
    pub fn from_snapshot(snapshot: crate::market::MarketDepthSnapshot) -> Self {
        let bids: Vec<PriceLevelVO> = snapshot
            .bids
            .into_iter()
            .map(PriceLevelVO::from_price_level)
            .collect();

        let asks: Vec<PriceLevelVO> = snapshot
            .asks
            .into_iter()
            .map(PriceLevelVO::from_price_level)
            .collect();

        Self {
            bids,
            asks,
            timestamp: chrono::Utc::now().to_rfc3339(),
            total_bid_orders: snapshot.total_bid_orders,
            total_ask_orders: snapshot.total_ask_orders,
        }
    }
}

impl PriceLevelVO {
    /// Create from original PriceLevel
    pub fn from_price_level(level: PriceLevel) -> Self {
        Self {
            price: level.price,
            quantity: level.quantity,
            order_count: level.order_count,
        }
    }
}

/// Engine Status View Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlfeStatusVO {
    pub orders_processed: u64,
    pub orders_matched: u64,
    pub total_quantity: f64,
    pub avg_match_latency_us: f64,
    pub peak_tps: u64,
    pub current_queue_depth: usize,
    pub last_update: String,
    pub price_volatility: f64,
    pub last_price: f64,
    pub last_price_update: String,
    pub match_rate: f64,
    pub current_tps: u64,
}

impl SlfeStatusVO {
    /// Create from original SlfeStatus
    pub fn from_status(status: &crate::matchengine::slfe::manager::status::SlfeStatus) -> Self {
        let match_rate = if status.orders_processed > 0 {
            status.orders_matched as f64 / status.orders_processed as f64
        } else {
            0.0
        };
        let last_update = {
            let system_time = SystemTime::now() - status.last_update.elapsed();
            system_time
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        };
        let last_price_update = {
            let system_time = SystemTime::now() - status.last_price_update.elapsed();
            system_time
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        };
        Self {
            orders_processed: status.orders_processed,
            orders_matched: status.orders_matched,
            total_quantity: status.total_quantity,
            avg_match_latency_us: status.avg_match_latency_us,
            peak_tps: status.peak_tps,
            current_queue_depth: status.current_queue_depth,
            last_update: last_update.to_string(),
            price_volatility: status.price_volatility,
            last_price: crate::matchengine::tool::math::atomic_to_f64(status.last_price_bits),
            last_price_update: last_price_update.to_string(),
            match_rate,
            current_tps: 0,
        }
    }
}
