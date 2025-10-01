use core::task;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use tokio::{join, task::JoinHandle};

use crate::{
    http::OrderBookHttpService,
    matcher::Matcher,
    orderbook::{OrderBook, OrderSourceChannel},
};

#[derive(Clone)]
pub struct LuminEngine {
    // key : trade symbol
    orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>,
}

impl LuminEngine {
    pub fn new() -> Self {
        Self {
            orderbooks: Arc::new(RwLock::new(
                HashMap::<String, Arc<RwLock<OrderBook>>>::default(),
            )),
        }
    }
    pub async fn startup(&self, matcher: Matcher, orderchannel: Vec<OrderSourceChannel>) {
        let mut tasks: Vec<JoinHandle<()>> = Vec::new();
        orderchannel.iter().for_each(|c| match c {
            OrderSourceChannel::Http => {
                let orderbooks = Arc::clone(&self.orderbooks);
                tasks.push(tokio::spawn(async move {
                    OrderBookHttpService::enable(orderbooks).await;
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
        // background service
        let orderbooks = Arc::clone(&self.orderbooks);
        tasks.push(tokio::spawn(BGService::enable(orderbooks)));
        for t in tasks {
            join!(t);
        }
    }
}

/// lumin engine background service, the service will continue to run until the process ends.
/// usually used to control the declaration lifecycle of a program.
struct BGService {}
impl BGService {
    pub async fn enable(orderbooks: Arc<RwLock<HashMap<String, Arc<RwLock<OrderBook>>>>>) {
        loop {}
    }
}
