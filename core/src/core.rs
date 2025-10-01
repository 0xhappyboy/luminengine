use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use tokio::{join, task::JoinHandle};

use crate::{
    http::OrderBookHttpService,
    matcher::Matcher,
    orderbook::{OrderBook, OrderSourceChannel},
};

pub struct LuminEngine {
    // key : trade symbol
    orderbooks: Arc<RwLock<HashMap<String, OrderBook>>>,
}
impl LuminEngine {
    pub fn new() -> Self {
        Self {
            orderbooks: Arc::new(RwLock::new(HashMap::<String, OrderBook>::default())),
        }
    }
    pub async fn startup<T>(&self, match_engine: T, orderchannel: Vec<OrderSourceChannel>)
    where
        T: Matcher,
    {
        let mut tasks: Vec<JoinHandle<()>> = Vec::new();
        orderchannel.iter().for_each(|c| match c {
            OrderSourceChannel::Http => {
                let en = Arc::clone(&self.orderbooks);
                tasks.push(tokio::spawn(async move {
                    OrderBookHttpService::enable(en);
                }));
            }
            OrderSourceChannel::Tcp => {
                // handle orders placed via the tcp protocol.
                todo!()
            }
            OrderSourceChannel::Rcp => {
                // handle orders placed via the rcp.
                todo!()
            }
        });
        for t in tasks {
            join!(t);
        }
    }
}
