use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crossbeam::channel::{Receiver, Sender, unbounded};

use crate::{
    matchengine::{
        MatchEvent, MatchResult,
        slfe::{
            self, Slfe,
            manager::event,
            processor::{OrderProcessor, limit::LimitOrderProcessor},
        },
        tool::math::f64_to_atomic,
    },
    types::{UnifiedError, UnifiedResult},
};

use std::sync::atomic::Ordering;

#[derive(Debug)]
pub struct EventManager {
    tx: Sender<MatchEvent>,
    rx: Arc<Receiver<MatchEvent>>,
}

impl EventManager {
    /// create event manager
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            tx: tx,
            rx: Arc::new(rx),
        }
    }
    /// start event engine
    pub async fn start_event_manager(&self, slfe: Arc<Slfe>) {
        let mut event_batch = Vec::<MatchEvent>::with_capacity(slfe.config.read().batch_size);
        let mut last_process_time = Instant::now();
        let process_interval = Duration::from_micros(slfe.config.read().match_interval);
        // slfe arc clone
        let slfe_arc_clone = Arc::clone(&slfe);
        loop {
            while let Ok(event) = slfe_arc_clone.event_manager.rx.try_recv() {
                event_batch.push(event);
                if event_batch.len() >= slfe_arc_clone.config.read().batch_size {
                    Self::handle_event_batch(slfe_arc_clone.as_ref(), &event_batch).await;
                    event_batch.clear();
                    last_process_time = Instant::now();
                }
            }
            if !event_batch.is_empty() && last_process_time.elapsed() >= process_interval {
                Self::handle_event_batch(slfe_arc_clone.as_ref(), &event_batch).await;
                event_batch.clear();
                last_process_time = Instant::now();
            }
            if event_batch.is_empty() {
                Self::try_continuous_match(slfe_arc_clone.as_ref()).await;
            }
            tokio::time::sleep(Duration::from_micros(10)).await;
        }
    }

    async fn try_continuous_match(slfe: &Slfe) {
        let mut match_occurred = true;
        let mut total_matched = 0;
        while match_occurred {
            match_occurred = false;
            if let Some(results) = LimitOrderProcessor::handle(slfe).await {
                if !results.is_empty() {
                    match_occurred = true;
                    total_matched += results.len();
                    // update current price and last match price
                    if let Some(last_price) = results.last().map(|r| r.price) {
                        slfe.current_price
                            .store(f64_to_atomic(last_price), Ordering::Relaxed);
                        slfe.last_match_price
                            .store(f64_to_atomic(last_price), Ordering::Relaxed);
                    }
                    Self::notify_match_results(slfe, results).await;
                }
            }
            if total_matched > 1000 {
                break;
            }
        }
        // update mid price
        if let Some(mid_price) = slfe.calculate_mid_price() {
            slfe.mid_price
                .store(f64_to_atomic(mid_price), Ordering::Relaxed);
        }
    }

    async fn handle_event_batch(slfe: &Slfe, events: &[MatchEvent]) {
        let start_time = Instant::now();
        let mut processed = 0;
        let mut matched = 0;
        let mut total_quantity = 0.0;
        for event in events {
            match event {
                MatchEvent::NewLimitOrder => {
                    processed += 1;
                    if let Some(results) = LimitOrderProcessor::handle(slfe).await {
                        matched += results.len();
                        total_quantity += results.iter().map(|r| r.quantity).sum::<f64>();
                        Self::notify_match_results(slfe, results).await;
                    }
                }
                MatchEvent::CancelOrder(order_id) => {
                    processed += 1;
                    OrderProcessor::handle_cancel_order(slfe, order_id).await;
                }
                MatchEvent::ImmediateMatch => {
                    if let Some(results) = LimitOrderProcessor::handle(slfe).await {
                        matched += results.len();
                        total_quantity += results.iter().map(|r| r.quantity).sum::<f64>();
                        Self::notify_match_results(slfe, results).await;
                    }
                }
                MatchEvent::UpdateConfig {
                    batch_size: _,
                    match_interval: _,
                } => {
                    // Matching engine configuration update event, currently no implementation.
                }
                MatchEvent::Shutdown => {
                    return;
                }
            }
        }
        slfe.status_manager.update(
            slfe,
            processed,
            matched,
            total_quantity,
            start_time.elapsed(),
        );
    }

    /// Test function, may be deleted in the future.
    async fn notify_match_results(slfe: &Slfe, results: Vec<MatchResult>) {
        for result in results {
            if result.quantity > 0.0 {
                slfe.current_price
                    .store(f64_to_atomic(result.price), Ordering::Relaxed);
                slfe.last_match_price
                    .store(f64_to_atomic(result.price), Ordering::Relaxed);
                if let Some(mid_price) = slfe.calculate_mid_price() {
                    slfe.mid_price
                        .store(f64_to_atomic(mid_price), Ordering::Relaxed);
                }
                println!(
                    "Matched: {} @ {} (bid: {}, ask: {})",
                    result.quantity,
                    result.price,
                    &result.bid_order_id[..8],
                    &result.ask_order_id[..8]
                );
            }
        }
    }

    /// send event
    pub fn send_event(&self, event: MatchEvent) -> UnifiedResult<String> {
        if self.tx.send(event).is_err() {
            return Err(UnifiedError::EventSendError("Event Send Error".to_string()));
        }
        Ok("Event Send successfully".to_string())
    }

    /// get the number of pending events
    pub fn get_pending_events(&self) -> usize {
        self.rx.len()
    }
}
