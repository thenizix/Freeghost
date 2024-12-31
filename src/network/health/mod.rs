// src/network/health/mod.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use tracing::{info, warn, error};
use thiserror::Error;

use crate::network::load_balancer::{NodeHealth, LoadBalancer};
use crate::utils::metrics::MetricsCollector;
use crate::utils::error::Result;

#[derive(Debug, Error)]
pub enum HealthMonitorError {
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
    #[error("Node timeout")]
    NodeTimeout,
    #[error("Metrics collection failed")]
    MetricsError,
    #[error("System-wide degradation detected")]
    SystemDegradation,
}

#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub check_interval: Duration,
    pub timeout: Duration,
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub metrics_window: Duration,
    pub latency_threshold: u64,
    pub error_rate_threshold: f64,
    pub load_threshold: f64,
    pub memory_threshold: f64,
    pub cpu_threshold: f64,
}

pub struct HealthMonitor {
    load_balancer: Arc<LoadBalancer>,
    metrics_collector: MetricsCollector,
    config: HealthCheckConfig,
    node_states: Arc<RwLock<HashMap<String, NodeState>>>,
    anomaly_detector: AnomalyDetector,
}

#[derive(Debug, Clone)]
struct NodeState {
    consecutive_failures: u32,
    consecutive_successes: u32,
    last_check: std::time::SystemTime,
    degradation_history: Vec<DegradationEvent>,
    performance_baseline: PerformanceBaseline,
}

#[derive(Debug, Clone)]
struct DegradationEvent {
    timestamp: std::time::SystemTime,
    metric_type: MetricType,
    value: f64,
    threshold: f64,
}

#[derive(Debug, Clone)]
struct PerformanceBaseline {
    avg_latency: f64,
    avg_error_rate: f64,
    avg_load: f64,
    std_dev_latency: f64,
    std_dev_error_rate: f64,
    std_dev_load: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum MetricType {
    Latency,
    ErrorRate,
    Load,
    Memory,
    CPU,
}

struct AnomalyDetector {
    detection_window: Duration,
    sensitivity: f64,
}

impl HealthMonitor {
    pub fn new(
        load_balancer: Arc<LoadBalancer>,
        metrics_collector: MetricsCollector,
        config: HealthCheckConfig,
    ) -> Self {
        Self {
            load_balancer,
            metrics_collector,
            config,
            node_states: Arc::new(RwLock::new(HashMap::new())),
            anomaly_detector: AnomalyDetector {
                detection_window: Duration::from_secs(300),
                sensitivity: 2.0,
            },
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut check_interval = interval(self.config.check_interval);
        let monitor = Arc::new(self.clone());

        tokio::spawn(async move {
            loop {
                check_interval.tick().await;
                if let Err(e) = monitor.run_health_checks().await {
                    error!("Health check error: {}", e);
                }
            }
        });

        Ok(())
    }

    async fn run_health_checks(&self) -> Result<()> {
        let nodes = self.load_balancer.get_all_nodes().await?;
        let mut system_health_metrics = Vec::new();
        
        for node_id in nodes {
            let metrics = self.check_node_health(&node_id).await;
            match metrics {
                Ok(health_metrics) => {
                    system_health_metrics.push(health_metrics.clone());
                    self.handle_node_success(&node_id).await?;
                    self.update_node_health(&node_id, health_metrics).await?;
                }
                Err(e) => {
                    warn!("Health check failed for node {}: {}", node_id, e);
                    self.handle_node_failure(&node_id).await?;
                }
            }
        }

        // Analyze system-wide health
        self.analyze_system_health(&system_health_metrics).await?;
        
        Ok(())
    }

    async fn check_node_health(&self, node_id: &str) -> Result<NodeHealth> {
        let latency = self.check_latency(node_id).await?;
        let error_rate = self.check_error_rate(node_id).await?;
        let (load, memory, cpu) = self.check_resource_usage(node_id).await?;

        // Check against thresholds
        if latency > self.config.latency_threshold {
            self.record_degradation(node_id, MetricType::Latency, latency as f64).await;
        }
        if error_rate > self.config.error_rate_threshold {
            self.record_degradation(node_id, MetricType::ErrorRate, error_rate).await;
        }
        if load > self.config.load_threshold {
            self.record_degradation(node_id, MetricType::Load, load).await;
        }

        Ok(NodeHealth {
            latency,
            error_rate,
            capacity: 100,  // This should be configurable
            current_load: (load * 100.0) as u32,
            zone: "default".to_string(),  // This should come from configuration
            last_failure: None,
            consecutive_failures: 0,
        })
    }

    async fn check_latency(&self, node_id: &str) -> Result<u64> {
        let start = std::time::Instant::now();
        
        // Send health check ping
        if let Err(e) = self.send_health_check_ping(node_id).await {
            return Err(e.into());
        }
        
        Ok(start.elapsed().as_millis() as u64)
    }

    async fn check_error_rate(&self, node_id: &str) -> Result<f64> {
        let metrics = self.metrics_collector.get_node_metrics(node_id).await?;
        Ok(metrics.error_rate)
    }

    async fn check_resource_usage(&self, node_id: &str) -> Result<(f64, f64, f64)> {
        let metrics = self.metrics_collector.get_resource_metrics(node_id).await?;
        Ok((metrics.load, metrics.memory_usage, metrics.cpu_usage))
    }

    async fn analyze_system_health(&self, metrics: &[NodeHealth]) -> Result<()> {
        if metrics.is_empty() {
            return Err(HealthMonitorError::SystemDegradation.into());
        }

        // Calculate system-wide metrics
        let avg_latency = metrics.iter().map(|m| m.latency as f64).sum::<f64>() / metrics.len() as f64;
        let avg_error_rate = metrics.iter().map(|m| m.error_rate).sum::<f64>() / metrics.len() as f64;
        let avg_load = metrics.iter().map(|m| m.current_load as f64).sum::<f64>() / metrics.len() as f64;

        // Check for system-wide issues
        if avg_latency > self.config.latency_threshold as f64 * 1.5 ||
           avg_error_rate > self.config.error_rate_threshold * 1.5 ||
           avg_load > self.config.load_threshold * 1.5 {
            error!("System-wide degradation detected");
            return Err(HealthMonitorError::SystemDegradation.into());
        }

        Ok(())
    }

    async fn record_degradation(&self, node_id: &str, metric_type: MetricType, value: f64) {
        let mut states = self.node_states.write().await;
        if let Some(state) = states.get_mut(node_id) {
            state.degradation_history.push(DegradationEvent {
                timestamp: std::time::SystemTime::now(),
                metric_type,
                value,
                threshold: match metric_type {
                    MetricType::Latency => self.config.latency_threshold as f64,
                    MetricType::ErrorRate => self.config.error_rate_threshold,
                    MetricType::Load => self.config.load_threshold,
                    MetricType::Memory => self.config.memory_threshold,
                    MetricType::CPU => self.config.cpu_threshold,
                },
            });

            // Remove old events
            state.degradation_history.retain(|event| {
                event.timestamp.elapsed().unwrap_or_default() < self.config.metrics_window
            });
        }
    }

    async fn send_health_check_ping(&self, node_id: &str) -> Result<()> {
        // Implement actual health check ping here
        // This is a placeholder that simulates a ping
        tokio::time::sleep(Duration::from_millis(5)).await;
        Ok(())
    }

    async fn handle_node_success(&self, node_id: &str) -> Result<()> {
        let mut states = self.node_states.write().await;
        if let Some(state) = states.get_mut(node_id) {
            state.consecutive_failures = 0;
            state.consecutive_successes += 1;
            state.last_check = std::time::SystemTime::now();

            if state.consecutive_successes >= self.config.success_threshold {
                info!("Node {} has recovered", node_id);
                self.load_balancer.mark_node_healthy(node_id).await?;
            }
        }
        Ok(())
    }

    async fn handle_node_failure(&self, node_id: &str) -> Result<()> {
        let mut states = self.node_states.write().await;
        if let Some(state) = states.get_mut(node_id) {
            state.consecutive_failures += 1;
            state.consecutive_successes = 0;
            state.last_check = std::time::SystemTime::now();

            if state.consecutive_failures >= self.config.failure_threshold {
                warn!("Node {} has failed health check threshold", node_id);
                self.load_balancer.mark_node_unhealthy(node_id).await?;
            }
        }
        Ok(())
    }

    async fn update_node_health(&self, node_id: &str, health: NodeHealth) -> Result<()> {
        self.load_balancer.update_node_health(node_id, health).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_monitoring() {
        let config = HealthCheckConfig {
            check_interval: Duration::from_secs(1),
            timeout: Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 2,
            metrics_window: Duration::from_secs(300),
            latency_threshold: 100,
            error_rate_threshold: 0.1,
            load_threshold: 0.8,
            memory_threshold: 0.9,
            cpu_threshold: 0.9,
        };

        // Create test dependencies
        let load_balancer = Arc::new(LoadBalancer::new(/* ... */));
        let metrics_collector = MetricsCollector::new();

        let monitor = HealthMonitor::new(
            load_balancer,
            metrics_collector,
            config,
        );

        // Start monitoring
        monitor.start().await.unwrap();

        // Wait for some health checks to run
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}