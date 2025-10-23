pub mod day;
pub mod fok;
pub mod gtc;
pub mod iceberg;
pub mod ioc;
pub mod limit;
pub mod market;
pub mod soptlimit;
pub mod stop;

use std::time::Instant;

use crate::{
    matchengine::{
        MatchEvent,
        slfe::{
            Slfe,
            processor::{
                day::DAYOrderProcessor, fok::FOKOrderProcessor, gtc::GTCOrderProcessor,
                iceberg::IcebergOrderProcessor, ioc::IOCOrderProcessor,
                market::MarketOrderProcessor, soptlimit::StopLimitOrderProcessor,
                stop::StopOrderProcessor,
            },
        },
        tool,
    },
    order::{Order, OrderDirection},
    types::{UnifiedError, UnifiedResult},
};

pub struct OrderProcessor;

impl OrderProcessor {
    /// a unified entry point for new orders.
    pub async fn handle_new_order(slfe: &Slfe, order: Order) -> UnifiedResult<String> {
        // verify order
        let order_verify = order.verify();
        if order_verify.is_err() {
            return order_verify;
        }
        // handle different types of orders
        match order.order_type {
            crate::order::OrderType::Limit => {
                let start_time = Instant::now();
                let location = match order.direction {
                    OrderDirection::Buy => slfe.bids.add_order(order.clone()),
                    OrderDirection::Sell => slfe.asks.add_order(order.clone()),
                    OrderDirection::None => {
                        return Err(UnifiedError::AddLimitOrderError(format!(
                            "Order Direction ERROR"
                        )));
                    }
                };
                slfe.order_location.insert(order.id.clone(), location);
                // send event
                if let Err(e) = slfe.tx.send(MatchEvent::NewLimitOrder) {
                    return Err(UnifiedError::AddLimitOrderError(format!(
                        "Event queue full: {}",
                        e
                    )));
                }
                // update status
                slfe.update_stats(1, 0, 0.0, start_time.elapsed());
                return Ok("".to_string());
            }
            crate::order::OrderType::Market => {
                MarketOrderProcessor::handle_market_order(slfe, order).await;
                return Ok("".to_string());
            }
            crate::order::OrderType::Stop => {
                StopOrderProcessor::handle_stop_order(slfe, order).await;
                return Ok("".to_string());
            }
            crate::order::OrderType::StopLimit => {
                StopLimitOrderProcessor::handle_stop_limit_order(slfe, order).await;
                return Ok("".to_string());
            }
            crate::order::OrderType::FOK => {
                FOKOrderProcessor::handle(slfe, order).await;
                return Ok("".to_string());
            }
            crate::order::OrderType::IOC => {
                IOCOrderProcessor::handle_ioc_order(slfe, order).await;
                return Ok("".to_string());
            }
            crate::order::OrderType::Iceberg => {
                IcebergOrderProcessor::handle(slfe, order).await;
                return Ok("".to_string());
            }
            crate::order::OrderType::DAY => {
                DAYOrderProcessor::handle(slfe, order, tool::expiry::expiry_today_end()).await;
                return Ok("".to_string());
            }
            crate::order::OrderType::GTC => GTCOrderProcessor::handle(slfe, order).await,
        }
    }
}
