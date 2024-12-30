// src/utils/monitoring.rs
use crate::utils::metrics::Metrics;
use std::sync::Arc;

pub struct Monitor {
    metrics: Arc<Metrics>,
    log_interval: Duration,
}

impl Monitor {
    pub async fn start(&self) {
        let metrics = self.metrics.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(self.log_interval).await;
                self.log_metrics(&metrics);
            }
        });
    }
    
    fn log_metrics(&self, metrics: &Metrics) {
        tracing::info!(
            total_requests = metrics.requests_total.load(Ordering::SeqCst),
            failed_requests = metrics.requests_failed.load(Ordering::SeqCst),
            avg_process_time = self.calculate_avg_time(metrics),
            "System metrics"
        );
    }
}
