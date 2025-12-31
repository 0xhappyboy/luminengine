pub mod config;
pub mod core;
pub mod global;
pub mod market;
pub mod matchengine;
pub mod monitor;
pub mod net;
pub mod order;
pub mod orderbook;
pub mod price;
pub mod sys;
pub mod target;
pub mod types;

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use crate::{core::LuminEngine, orderbook::OrderSourceChannel};

    #[tokio::test]
    async fn test() {
        LuminEngine::startup(vec![OrderSourceChannel::Http, OrderSourceChannel::Rcp]).await;
    }
}
