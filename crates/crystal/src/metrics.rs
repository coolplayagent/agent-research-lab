use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

#[derive(Default)]
pub(crate) struct Metrics {
    pub accepted: AtomicU64,
    pub duplicates: AtomicU64,
    pub rejected: AtomicU64,
    pub lagged: AtomicU64,
    pub replayed: AtomicU64,
    pub failures: AtomicU64,
    pub queued: AtomicUsize,
    pub subscribers: AtomicUsize,
    pub peak_subscribers: AtomicUsize,
    pub ack_ms: Mutex<VecDeque<f64>>,
    pub commit_ms: Mutex<VecDeque<f64>>,
}
impl Metrics {
    pub fn sample(target: &Mutex<VecDeque<f64>>, value: f64) {
        let mut values = target.lock().unwrap();
        if values.len() == 4096 {
            values.pop_front();
        }
        values.push_back(value);
    }
    pub fn view(&self) -> Value {
        let percentile = |samples: &Mutex<VecDeque<f64>>| {
            let mut values: Vec<_> = samples.lock().unwrap().iter().copied().collect();
            values.sort_by(f64::total_cmp);
            let get = |p: f64| {
                values
                    .get(((values.len() as f64 * p).ceil() as usize).saturating_sub(1))
                    .copied()
            };
            json!({"samples":values.len(),"p50":get(0.5),"p95":get(0.95),"p99":get(0.99),"max":values.last()})
        };
        json!({"accepted":self.accepted.load(Ordering::Relaxed),"duplicates":self.duplicates.load(Ordering::Relaxed),"rejected":self.rejected.load(Ordering::Relaxed),"lag_events":self.lagged.load(Ordering::Relaxed),"replayed":self.replayed.load(Ordering::Relaxed),"storage_failures":self.failures.load(Ordering::Relaxed),"queue_depth":self.queued.load(Ordering::Relaxed),"subscriptions":self.subscribers.load(Ordering::Relaxed),"peak_subscriptions":self.peak_subscribers.load(Ordering::Relaxed),"durable_ack_ms":percentile(&self.ack_ms),"commit_ms":percentile(&self.commit_ms),"latency_scope":"host ingress through durable commit; subscriber/network/UI latency requires an external observation"})
    }
}
