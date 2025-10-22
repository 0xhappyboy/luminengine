use std::sync::{Arc, RwLock};

use crate::config::RPC_LISTENER_PORT;
use crate::order::OrderDirection;
use crate::orderbook::OrderBooks;
use crate::target::Target;
/// order book RPC service, RPC service for handling order books
use orderbook::order_book_service_server::{OrderBookService, OrderBookServiceServer};
use orderbook::{Order, OrderBook, OrderResponse};
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod orderbook {
    include!("protos/orderbook.rs");
}

#[derive(Debug, Default)]
pub struct OrderBookRPCService;

impl OrderBookRPCService {
    pub async fn enable() {
        let addr = RPC_LISTENER_PORT.lock().unwrap().parse().unwrap();
        let order_service = OrderBookServiceServer::new(OrderBookRPCService::default());
        Server::builder()
            .add_service(order_service)
            .serve(addr)
            .await
            .unwrap();
    }
}

#[tonic::async_trait]
impl OrderBookService for OrderBookRPCService {
    async fn create_order_book(
        &self,
        request: Request<OrderBook>,
    ) -> Result<Response<OrderResponse>, Status> {
        let orderbook = request.into_inner();
        let symbol = orderbook.symbol;
        let response;
        match crate::orderbook::OrderBooks::insert(
            symbol.clone(),
            Arc::new(RwLock::new(crate::orderbook::OrderBook::new(Target {
                symbol: symbol.clone(),
            }))),
        ) {
            Ok(s) => {
                response = OrderResponse {
                    id: 1,
                    message: format!("{}", s),
                };
                Ok(Response::new(response))
            }
            Err(e) => Err(Status::new(
                tonic::Code::FailedPrecondition,
                format!("{}", e).to_string(),
            )),
        }
    }
    // buy order
    async fn buy(&self, request: Request<Order>) -> Result<Response<OrderResponse>, Status> {
        let order: crate::net::rpc::server::orderbook::Order = request.into_inner();
        handle_order(order, OrderDirection::Buy)
    }
    // sell order
    async fn sell(&self, request: Request<Order>) -> Result<Response<OrderResponse>, Status> {
        let order: crate::net::rpc::server::orderbook::Order = request.into_inner();
        handle_order(order, OrderDirection::Sell)
    }
    // cancel order
    async fn cancel(&self, request: Request<Order>) -> Result<Response<OrderResponse>, Status> {
        let order = request.into_inner();
        let response = OrderResponse {
            id: 1,
            message: format!("Order {}", 1),
        };
        Ok(Response::new(response))
    }
}
fn param_verif(order: crate::net::rpc::server::orderbook::Order) -> Result<(), Status> {
    if order.symbol.is_empty() {
        return Err(Status::new(
            tonic::Code::FailedPrecondition,
            format!("symbol cannot be empty").to_string(),
        ));
    }
    if order.price <= 0.0 {
        return Err(Status::new(
            tonic::Code::FailedPrecondition,
            format!("price is illegal").to_string(),
        ));
    }
    Ok(())
}

fn handle_order(
    order: crate::net::rpc::server::orderbook::Order,
    order_direction: OrderDirection,
) -> Result<Response<OrderResponse>, Status> {
    let response;
    match param_verif(order.clone()) {
        Ok(_) => match OrderBooks::get_orderbook_by_symbol(order.symbol.clone()) {
            Some(orderbook) => {
                orderbook
                    .write()
                    .unwrap()
                    .push_order(crate::order::Order::from_rpc_order(
                        order.clone(),
                        order_direction,
                    ));
                response = OrderResponse {
                    id: 1,
                    message: format!("Order {}", 1),
                };
                Ok(Response::new(response))
            }
            None => Err(Status::new(
                tonic::Code::FailedPrecondition,
                format!("orderbook does not exist").to_string(),
            )),
        },
        Err(e) => {
            return Err(e);
        }
    }
}
