use std::sync::{Arc, Mutex};

use luminengine::{
    core::LuminEngine,
    orderbook::{OrderBook, OrderSourceChannel},
};

#[tokio::main]
async fn main() {
    LuminEngine::startup(vec![OrderSourceChannel::Http, OrderSourceChannel::Rcp]).await;
}
