/// order book RPC service, RPC service for handling order books
use orderbook::order_book_service_server::{OrderBookService, OrderBookServiceServer};
use orderbook::{Order, OrderBook, OrderResponse};
use tonic::{Request, Response, Status};

pub mod orderbook {
    include!("protos/orderbook.rs");
}

#[derive(Debug, Default)]
pub struct OrderBookRPCService;

#[tonic::async_trait]
impl OrderBookService for OrderBookRPCService {
    async fn create_order_book(
        &self,
        request: Request<OrderBook>,
    ) -> Result<Response<OrderResponse>, Status> {
        let req = request.into_inner();
        let response = OrderResponse {
            id: 1,
            description: format!("Order {}", 1),
        };
        Ok(Response::new(response))
    }
    async fn buy(&self, request: Request<Order>) -> Result<Response<OrderResponse>, Status> {
        let req = request.into_inner();
        let response = OrderResponse {
            id: 1,
            description: format!("Order {}", 1),
        };
        Ok(Response::new(response))
    }
    async fn sell(&self, request: Request<Order>) -> Result<Response<OrderResponse>, Status> {
        let req = request.into_inner();
        let response = OrderResponse {
            id: 1,
            description: format!("Order {}", 1),
        };
        Ok(Response::new(response))
    }
    async fn cancel(&self, request: Request<Order>) -> Result<Response<OrderResponse>, Status> {
        let req = request.into_inner();
        let response = OrderResponse {
            id: 1,
            description: format!("Order {}", 1),
        };
        Ok(Response::new(response))
    }
}
// startup rpc server
pub fn create_server() -> OrderBookServiceServer<OrderBookRPCService> {
    OrderBookServiceServer::new(OrderBookRPCService::default())
}
