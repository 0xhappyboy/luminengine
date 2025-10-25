use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crossbeam::channel::{Receiver, Sender, unbounded};

use crate::{
    matchengine::{
        MatchEvent,
        slfe::{
            Slfe,
            processor::{OrderProcessor, limit::LimitOrderProcessor},
        },
        tool::math::f64_to_atomic,
    },
    types::{UnifiedError, UnifiedResult},
};

use std::sync::atomic::Ordering;

const MAX_CONTINUOUS_MATCH_LIMIT: usize = 1000;
const EVENT_MANAGER_SLEEP_MICROS: Duration = Duration::from_micros(10);

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
        // slfe arc clone
        let slfe_arc_clone = Arc::clone(&slfe);
        loop {
            // Hold the lock briefly to read dynamic configuration changes.
            let (batch_size, match_interval) = {
                let config = slfe.config.read();
                (config.batch_size, config.match_interval)
            };
            // Dynamically obtain configuration
            let process_interval = Duration::from_micros(match_interval);
            while let Ok(event) = slfe_arc_clone.event_manager.rx.try_recv() {
                event_batch.push(event);
                if event_batch.len() >= batch_size {
                    Self::handle_event_batch(slfe_arc_clone.clone(), &event_batch).await;
                    event_batch.clear();
                    last_process_time = Instant::now();
                }
            }
            if !event_batch.is_empty() && last_process_time.elapsed() >= process_interval {
                Self::handle_event_batch(slfe_arc_clone.clone(), &event_batch).await;
                event_batch.clear();
                last_process_time = Instant::now();
            }
            // Assume that there are no events, execute the logic.
            if event_batch.is_empty() {
                Self::try_continuous_match(slfe_arc_clone.clone()).await;
            }
            tokio::time::sleep(EVENT_MANAGER_SLEEP_MICROS).await;
        }
    }

    async fn try_continuous_match(slfe: Arc<Slfe>) {
        let mut match_occurred = true;
        let mut total_matched = 0;
        while match_occurred {
            match_occurred = false;
            if let Some(results) = LimitOrderProcessor::handle(slfe.as_ref()).await {
                if !results.is_empty() {
                    match_occurred = true;
                    total_matched += results.len();
                    // update current price and last match price
                    if let Some(last_price) = results.last().map(|r| r.price) {
                        slfe.price_info_manager
                            .current_price
                            .store(f64_to_atomic(last_price), Ordering::Relaxed);
                        slfe.price_info_manager
                            .last_match_price
                            .store(f64_to_atomic(last_price), Ordering::Relaxed);
                    }
                    slfe.price_info_manager
                        .update_price(slfe.clone(), results)
                        .await;
                }
            }
            if total_matched > MAX_CONTINUOUS_MATCH_LIMIT {
                break;
            }
        }
        // update mid price
        if let Some(mid_price) = slfe.price_info_manager.cal_mid_price(slfe.clone()) {
            slfe.price_info_manager
                .mid_price
                .store(f64_to_atomic(mid_price), Ordering::Relaxed);
        }
    }

    async fn handle_event_batch(slfe: Arc<Slfe>, events: &[MatchEvent]) {
        let start_time = Instant::now();
        let mut processed = 0;
        let mut matched = 0;
        let mut total_quantity = 0.0;
        for event in events {
            match event {
                MatchEvent::NewLimitOrder => {
                    processed += 1;
                    if let Some(results) = LimitOrderProcessor::handle(slfe.as_ref()).await {
                        matched += results.len();
                        total_quantity += results.iter().map(|r| r.quantity).sum::<f64>();
                        slfe.price_info_manager
                            .update_price(slfe.clone(), results)
                            .await;
                    }
                }
                MatchEvent::CancelOrder(order_id) => {
                    processed += 1;
                    OrderProcessor::handle_cancel_order(slfe.as_ref(), order_id).await;
                }
                MatchEvent::ImmediateMatch => {
                    if let Some(results) = LimitOrderProcessor::handle(slfe.as_ref()).await {
                        matched += results.len();
                        total_quantity += results.iter().map(|r| r.quantity).sum::<f64>();
                        slfe.price_info_manager
                            .update_price(slfe.clone(), results)
                            .await;
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
            slfe.as_ref(),
            processed,
            matched,
            total_quantity,
            start_time.elapsed(),
        );
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
