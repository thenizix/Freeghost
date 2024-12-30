// src/core/services/health.rs
use std::sync::atomic::{AtomicU64, Ordering};

pub struct HealthService {
    start_time: i64,
    processed_requests: AtomicU64,
}

impl HealthService {
    pub fn new() -> Self {
        Self {
            start_time: chrono::Utc::now().timestamp(),
            processed_requests: AtomicU64::new(0),
        }
    }

    pub fn get_metrics(&self) -> HealthMetrics {
        HealthMetrics {
            uptime: chrono::Utc::now().timestamp() - self.start_time,
            processed_requests: self.processed_requests.load(Ordering::Relaxed),
        }
    }
}