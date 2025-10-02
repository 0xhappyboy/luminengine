use std::sync::{Arc, Mutex};

use luminengine::{
    core::LuminEngine,
    matcher::Matcher,
    orderbook::{OrderBook, OrderSourceChannel},
};

#[tokio::main]
async fn main() {
    LuminEngine::new()
        .startup(vec![OrderSourceChannel::Http])
        .await;
}
