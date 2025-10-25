use std::sync::Arc;

use crate::{
    matchengine::{
        MatchResult,
        slfe::{Slfe, processor::limit::LimitOrderProcessor},
    },
    order::{Order, OrderDirection, OrderType},
    price::Price,
    types::UnifiedResult,
};

pub struct IOCOrderProcessor;

impl IOCOrderProcessor {
    pub async fn handle_ioc_order(
        slfe: Arc<Slfe>,
        ioc_order: Order,
    ) -> UnifiedResult<Vec<MatchResult>> {
        Self::validate_ioc_order(&ioc_order)?;

        println!(
            "Processing IOC order {}: {} {} @ {}",
            &ioc_order.id[..8],
            ioc_order.quantity,
            match ioc_order.direction {
                OrderDirection::Buy => "BUY",
                OrderDirection::Sell => "SELL",
                OrderDirection::None => "NONE",
            },
            ioc_order.price
        );
        let results = Self::match_immediately(slfe.clone(), &ioc_order).await;
        Self::process_ioc_result(&ioc_order, &results);
        Ok(results)
    }

    async fn match_immediately(slfe: Arc<Slfe>, ioc_order: &Order) -> Vec<MatchResult> {
        let mut results = Vec::new();
        let mut remaining_quantity = ioc_order.quantity;

        match ioc_order.direction {
            OrderDirection::Buy => {
                let ask_prices = slfe.asks.get_all_ask_prices_sorted_asc();
                for ask_price in ask_prices {
                    if ask_price.to_f64() > ioc_order.price {
                        break;
                    }

                    if remaining_quantity <= 0.0 {
                        break;
                    }
                    for shard_id in 0..slfe.config.read().shard_count {
                        let shard = slfe.asks.shards[shard_id].read();
                        if let Some(orders) = shard.tree.get(&ask_price) {
                            for counter_order in orders {
                                if !counter_order.can_trade() {
                                    continue;
                                }

                                let trade_quantity =
                                    remaining_quantity.min(counter_order.remaining);
                                if trade_quantity > 0.0 {
                                    results.push(MatchResult {
                                        bid_order_id: ioc_order.id.clone(),
                                        ask_order_id: counter_order.id.clone(),
                                        quantity: trade_quantity,
                                        price: ask_price.to_f64(),
                                        timestamp: std::time::Instant::now(),
                                    });

                                    remaining_quantity -= trade_quantity;

                                    if remaining_quantity <= 0.0 {
                                        break;
                                    }
                                }
                            }
                        }
                        if remaining_quantity <= 0.0 {
                            break;
                        }
                    }
                }
            }
            OrderDirection::Sell => {
                let bid_prices = slfe.bids.get_all_bid_prices_sorted_desc();
                for bid_price in bid_prices {
                    if bid_price.to_f64() < ioc_order.price {
                        break;
                    }

                    if remaining_quantity <= 0.0 {
                        break;
                    }
                    for shard_id in 0..slfe.config.read().shard_count {
                        let shard = slfe.bids.shards[shard_id].read();
                        if let Some(orders) = shard.tree.get(&bid_price) {
                            for counter_order in orders {
                                if !counter_order.can_trade() {
                                    continue;
                                }
                                let trade_quantity =
                                    remaining_quantity.min(counter_order.remaining);
                                if trade_quantity > 0.0 {
                                    results.push(MatchResult {
                                        bid_order_id: counter_order.id.clone(),
                                        ask_order_id: ioc_order.id.clone(),
                                        quantity: trade_quantity,
                                        price: bid_price.to_f64(),
                                        timestamp: std::time::Instant::now(),
                                    });

                                    remaining_quantity -= trade_quantity;

                                    if remaining_quantity <= 0.0 {
                                        break;
                                    }
                                }
                            }
                        }
                        if remaining_quantity <= 0.0 {
                            break;
                        }
                    }
                }
            }
            OrderDirection::None => {}
        }

        results
    }

    fn validate_ioc_order(order: &Order) -> UnifiedResult<()> {
        if order.order_type != OrderType::Limit {
            return Err(crate::types::UnifiedError::OrderError(
                "IOC orders must be limit orders".to_string(),
            ));
        }
        if order.price <= 0.0 {
            return Err(crate::types::UnifiedError::OrderError(
                "IOC orders must have a valid price".to_string(),
            ));
        }
        if order.quantity <= 0.0 {
            return Err(crate::types::UnifiedError::OrderError(
                "IOC orders must have a valid quantity".to_string(),
            ));
        }
        Ok(())
    }

    fn process_ioc_result(ioc_order: &Order, results: &[MatchResult]) {
        let total_filled: f64 = results.iter().map(|r| r.quantity).sum();

        if total_filled > 0.0 {
            if total_filled < ioc_order.quantity {
                let remaining_quantity = ioc_order.quantity - total_filled;
                println!(
                    "IOC order {}: PARTIALLY FILLED - {}/{} filled, {} CANCELLED",
                    &ioc_order.id[..8],
                    total_filled,
                    ioc_order.quantity,
                    remaining_quantity
                );
            } else {
                println!(
                    "IOC order {}: FULLY FILLED - {}",
                    &ioc_order.id[..8],
                    ioc_order.quantity
                );
            }
        } else {
            println!(
                "IOC order {}: NOT FILLED - entire order {} CANCELLED",
                &ioc_order.id[..8],
                ioc_order.quantity
            );
        }
    }

    pub async fn check_ioc_feasibility(slfe: &Slfe, order: &Order) -> (bool, f64) {
        let mut available_quantity = 0.0;

        match order.direction {
            OrderDirection::Buy => {
                let ask_prices = slfe.asks.get_all_ask_prices_sorted_asc();
                for ask_price in ask_prices {
                    if ask_price.to_f64() > order.price {
                        break;
                    }
                    available_quantity +=
                        slfe.asks.get_total_base_unit_by_price(ask_price.to_f64());
                }
            }
            OrderDirection::Sell => {
                let bid_prices = slfe.bids.get_all_bid_prices_sorted_desc();
                for bid_price in bid_prices {
                    if bid_price.to_f64() < order.price {
                        break;
                    }
                    available_quantity +=
                        slfe.bids.get_total_base_unit_by_price(bid_price.to_f64());
                }
            }
            OrderDirection::None => {}
        }

        let can_fully_fill = available_quantity >= order.quantity;
        (can_fully_fill, available_quantity)
    }
}
