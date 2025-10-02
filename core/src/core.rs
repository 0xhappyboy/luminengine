use std::sync::{Arc, RwLock};

use tokio::{join, task::JoinHandle};

use crate::{
    http::OrderBookHttpService,
    orderbook::{OrderBooks, OrderSourceChannel},
    rpc::server::OrderBookRPCService,
};

#[derive(Clone)]
pub struct LuminEngine;
impl LuminEngine {
    pub async fn startup(orderchannel: Vec<OrderSourceChannel>) {
        let mut tasks: Vec<JoinHandle<()>> = Vec::new();
        orderchannel.iter().for_each(|c| match c {
            OrderSourceChannel::Http => {
                tasks.push(tokio::spawn(async move {
                    OrderBookHttpService::enable().await;
                }));
            }
            OrderSourceChannel::Tcp => {
                // handle orders placed via the tcp protocol.
                todo!()
            }
            OrderSourceChannel::Rcp => {
                // handle orders placed via the rcp.
                tasks.push(tokio::spawn(async move {
                    OrderBookRPCService::enable().await;
                }));
            }
        });
        // background service
        tasks.push(tokio::spawn(BGService::enable()));
        for t in tasks {
            join!(t);
        }
    }
}

/// lumin engine background service, the service will continue to run until the process ends.
/// usually used to control the declaration lifecycle of a program.
struct BGService {}
impl BGService {
    pub async fn enable() {
        loop {}
    }
}
