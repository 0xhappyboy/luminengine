use std::sync::{Arc, Mutex};

use luminengine::{
    core::LuminEngine,
    matcher::Matcher,
    orderbook::{OrderBook, OrderSourceChannel},
};

#[tokio::main]
async fn main() {
    LuminEngine::startup(vec![OrderSourceChannel::Http, OrderSourceChannel::Rcp]).await;
}
