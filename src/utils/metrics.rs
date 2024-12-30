// src/utils/metrics.rs
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::{Duration, Instant};

pub struct Metrics {
    start_time: Instant,
    requests_total: AtomicU64,
    requests_failed: AtomicU64,
    processing_time: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            requests_total: AtomicU64::new(0),
            requests_failed: AtomicU64::new(0),
            processing_time: AtomicU64::new(0),
        }
    }

    pub fn record_request(&self, duration: Duration, success: bool) {
        self.requests_total.fetch_add(1, Ordering::SeqCst);
        self.processing_time.fetch_add(duration.as_micros() as u64, Ordering::SeqCst);
        if !success {
            self.requests_failed.fetch_add(1, Ordering::SeqCst);
        }
    }
}