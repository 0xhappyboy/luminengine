pub mod day;
pub mod fok;
pub mod gtc;
pub mod iceberg;
pub mod ioc;
pub mod limit;
pub mod market;
pub mod stop;
pub mod stoplimit;

use std::{sync::Arc, time::Instant};

use crate::{
    matchengine::{
        MatchEvent,
        slfe::{
            Slfe,
            processor::{
                day::DAYOrderProcessor, fok::FOKOrderProcessor, gtc::GTCOrderProcessor,
                iceberg::IcebergOrderProcessor, ioc::IOCOrderProcessor, limit::LimitOrderProcessor,
                market::MarketOrderProcessor,
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
    pub fn handle_new_order(slfe: Arc<Slfe>, order: Order) -> UnifiedResult<String> {
        // verify order
        let order_verify = order.verify();
        if order_verify.is_err() {
            return order_verify;
        }
        // handle different types of orders
        match order.order_type {
            crate::order::OrderType::Limit => {
                let slfe_arc_clone = slfe.clone();
                let _ = LimitOrderProcessor::add(slfe_arc_clone, order);
                return Ok("".to_string());
            }
            crate::order::OrderType::Market => {
                let slfe_arc_clone = slfe.clone();
                MarketOrderProcessor::handle(slfe_arc_clone, order);
                return Ok("".to_string());
            }
            crate::order::OrderType::Stop => {
                let slfe_arc_clone = slfe.clone();
                todo!();
                return Ok("".to_string());
            }
            crate::order::OrderType::StopLimit => {
                let slfe_arc_clone = slfe.clone();
                todo!();
                return Ok("".to_string());
            }
            crate::order::OrderType::FOK => {
                let slfe_arc_clone = slfe.clone();
                FOKOrderProcessor::handle(slfe_arc_clone, order);
                return Ok("".to_string());
            }
            crate::order::OrderType::IOC => {
                let slfe_arc_clone = slfe.clone();
                let _ = IOCOrderProcessor::handle_ioc_order(slfe_arc_clone, order);
                return Ok("".to_string());
            }
            crate::order::OrderType::Iceberg => {
                let slfe_arc_clone = slfe.clone();
                let _ = IcebergOrderProcessor::handle(slfe_arc_clone, order);
                return Ok("".to_string());
            }
            crate::order::OrderType::DAY => {
                let slfe_arc_clone = slfe.clone();
                let _ = DAYOrderProcessor::handle(
                    slfe_arc_clone,
                    order,
                    tool::expiry::expiry_today_end(),
                );
                return Ok("".to_string());
            }
            crate::order::OrderType::GTC => GTCOrderProcessor::handle(slfe, order),
        }
    }

    /// Unified order cancellation entry
    pub fn handle_cancel_order(slfe: &Slfe, order_id: &str) {
        if let Some(location) = slfe.order_location.get(order_id) {
            match location.direction {
                OrderDirection::Buy => {
                    slfe.bids.remove_order(order_id, location.shard_id);
                }
                OrderDirection::Sell => {
                    slfe.asks.remove_order(order_id, location.shard_id);
                }
                OrderDirection::None => (),
            }
            slfe.order_location.remove(order_id);
        }
    }
}
