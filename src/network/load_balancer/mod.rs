// src/network/load_balancer/mod.rs
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use thiserror::Error;

use crate::network::circuit_breaker::CircuitBreaker;
use crate::utils::error::Result;
use crate::utils::metrics::MetricsCollector;

#[derive(Debug, Clone, PartialEq)]
pub struct NodeHealth {
    pub latency: u64,
    pub error_rate: f64,
    pub capacity: u32,
    pub current_load: u32,
    pub zone: String,
    pub last_failure: Option<std::time::SystemTime>,
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone)]
pub struct NodeMetrics {
    pub node_id: String,
    pub health: NodeHealth,
    pub last_updated: std::time::SystemTime,
    pub weights: DynamicWeights,
}

#[derive(Debug, Clone)]
pub struct DynamicWeights {
    pub latency_weight: f64,
    pub error_weight: f64,
    pub load_weight: f64,
    pub zone_weight: f64,
}

#[derive(Debug, Clone)]
pub struct ZoneAwareness {
    preferred_zones: HashSet<String>,
    zone_latencies: HashMap<String, u64>,
}

#[derive(Debug, Error)]
pub enum LoadBalancerError {
    #[error("No healthy nodes available")]
    NoHealthyNodes,
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Circuit breaker open")]
    CircuitBreakerOpen,
    #[error("Health check failed")]
    HealthCheckFailed,
    #[error("Zone not available: {0}")]
    ZoneNotAvailable(String),
}

pub struct LoadBalancer {
    nodes: Arc<RwLock<HashMap<String, NodeMetrics>>>,
    circuit_breaker: CircuitBreaker,
    metrics_collector: MetricsCollector,
    health_threshold: NodeHealth,
    zone_awareness: Arc<RwLock<ZoneAwareness>>,
    weight_adjuster: Arc<RwLock<WeightAdjuster>>,
}

struct WeightAdjuster {
    history_window: Vec<NodeMetrics>,
    adjustment_threshold: f64,
}

impl LoadBalancer {
    pub fn new(
        circuit_breaker: CircuitBreaker,
        metrics_collector: MetricsCollector,
        health_threshold: NodeHealth,
        preferred_zones: HashSet<String>,
    ) -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            circuit_breaker,
            metrics_collector,
            health_threshold,
            zone_awareness: Arc::new(RwLock::new(ZoneAwareness {
                preferred_zones,
                zone_latencies: HashMap::new(),
            })),
            weight_adjuster: Arc::new(RwLock::new(WeightAdjuster {
                history_window: Vec::new(),
                adjustment_threshold: 0.1,
            })),
        }
    }

    pub async fn add_node(&self, node_id: String, initial_health: NodeHealth) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        let weights = DynamicWeights {
            latency_weight: 0.4,
            error_weight: 0.3,
            load_weight: 0.2,
            zone_weight: 0.1,
        };
        
        nodes.insert(
            node_id.clone(),
            NodeMetrics {
                node_id,
                health: initial_health.clone(),
                last_updated: std::time::SystemTime::now(),
                weights,
            },
        );

        // Update zone awareness
        let mut zone_awareness = self.zone_awareness.write().await;
        let zone_latencies = &mut zone_awareness.zone_latencies;
        zone_latencies
            .entry(initial_health.zone)
            .and_modify(|latency| *latency = (*latency + initial_health.latency) / 2)
            .or_insert(initial_health.latency);

        Ok(())
    }

    pub async fn get_next_node(&self) -> Result<String> {
        if self.circuit_breaker.is_open().await {
            return Err(LoadBalancerError::CircuitBreakerOpen.into());
        }

        let nodes = self.nodes.read().await;
        let zone_awareness = self.zone_awareness.read().await;
        
        // First try preferred zones
        let mut best_node = self.find_best_node_in_zones(
            &nodes,
            &zone_awareness.preferred_zones
        ).await;
        
        // If no node found in preferred zones, try all zones
        if best_node.is_none() {
            best_node = self.find_best_node_in_any_zone(&nodes).await;
        }

        best_node.ok_or_else(|| LoadBalancerError::NoHealthyNodes.into())
    }

    async fn find_best_node_in_zones(
        &self,
        nodes: &HashMap<String, NodeMetrics>,
        preferred_zones: &HashSet<String>,
    ) -> Option<String> {
        let mut best_node = None;
        let mut best_score = f64::MAX;

        for (node_id, metrics) in nodes.iter() {
            if preferred_zones.contains(&metrics.health.zone) && 
               self.is_node_healthy(&metrics.health) {
                let score = self.calculate_node_score(metrics).await;
                if score < best_score {
                    best_score = score;
                    best_node = Some(node_id.clone());
                }
            }
        }

        best_node
    }

    async fn find_best_node_in_any_zone(
        &self,
        nodes: &HashMap<String, NodeMetrics>,
    ) -> Option<String> {
        let mut best_node = None;
        let mut best_score = f64::MAX;

        for (node_id, metrics) in nodes.iter() {
            if self.is_node_healthy(&metrics.health) {
                let score = self.calculate_node_score(metrics).await;
                if score < best_score {
                    best_score = score;
                    best_node = Some(node_id.clone());
                }
            }
        }

        best_node
    }

    async fn calculate_node_score(&self, metrics: &NodeMetrics) -> f64 {
        let normalized_latency = metrics.health.latency as f64 / self.health_threshold.latency as f64;
        let normalized_load = metrics.health.current_load as f64 / metrics.health.capacity as f64;
        
        // Get zone penalty
        let zone_awareness = self.zone_awareness.read().await;
        let zone_penalty = if zone_awareness.preferred_zones.contains(&metrics.health.zone) {
            0.0
        } else {
            0.5
        };

        // Calculate weighted score
        (normalized_latency * metrics.weights.latency_weight)
            + (metrics.health.error_rate * metrics.weights.error_weight)
            + (normalized_load * metrics.weights.load_weight)
            + (zone_penalty * metrics.weights.zone_weight)
    }

    pub async fn adjust_weights(&self) -> Result<()> {
        let mut weight_adjuster = self.weight_adjuster.write().await;
        let nodes = self.nodes.read().await;

        // Add current metrics to history
        weight_adjuster.history_window.extend(nodes.values().cloned());

        // Keep only recent history
        if weight_adjuster.history_window.len() > 100 {
            weight_adjuster.history_window.drain(0..50);
        }

        // Analyze performance patterns
        for node_metrics in nodes.values_mut() {
            let performance = self.analyze_node_performance(
                node_metrics,
                &weight_adjuster.history_window
            ).await;

            // Adjust weights based on performance
            self.update_weights(node_metrics, &performance).await?;
        }

        Ok(())
    }

    async fn analyze_node_performance(
        &self,
        metrics: &NodeMetrics,
        history: &[NodeMetrics]
    ) -> NodePerformance {
        // Calculate performance metrics based on history
        let latency_trend = self.calculate_trend(history, |m| m.health.latency as f64);
        let error_trend = self.calculate_trend(history, |m| m.health.error_rate);
        let load_trend = self.calculate_trend(history, |m| m.health.current_load as f64);

        NodePerformance {
            latency_trend,
            error_trend,
            load_trend,
        }
    }

    fn calculate_trend<F>(&self, history: &[NodeMetrics], metric_fn: F) -> f64 
    where
        F: Fn(&NodeMetrics) -> f64
    {
        if history.is_empty() {
            return 0.0;
        }

        let values: Vec<f64> = history.iter().map(metric_fn).collect();
        let avg = values.iter().sum::<f64>() / values.len() as f64;
        
        // Calculate trend as deviation from average
        values.last().unwrap_or(&avg) - avg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_zone_aware_load_balancing() {
        let circuit_breaker = CircuitBreaker::new(
            Duration::from_secs(60),
            5,
            Duration::from_secs(300),
        );
        let metrics_collector = MetricsCollector::new();
        let health_threshold = NodeHealth {
            latency: 100,
            error_rate: 0.1,
            capacity: 1000,
            current_load: 0,
            zone: "default".to_string(),
            last_failure: None,
            consecutive_failures: 0,
        };

        let mut preferred_zones = HashSet::new();
        preferred_zones.insert("zone1".to_string());

        let lb = LoadBalancer::new(
            circuit_breaker,
            metrics_collector,
            health_threshold,
            preferred_zones,
        );

        // Add nodes in different zones
        lb.add_node(
            "node1".to_string(),
            NodeHealth {
                latency: 50,
                error_rate: 0.05,
                capacity: 1000,
                current_load: 500,
                zone: "zone1".to_string(),
                last_failure: None,
                consecutive_failures: 0,
            },
        )
        .await
        .unwrap();

        lb.add_node(
            "node2".to_string(),
            NodeHealth {
                latency: 30,  // Better latency
                error_rate: 0.03,  // Better error rate
                capacity: 1000,
                current_load: 300,  // Better load
                zone: "zone2".to_string(),  // But not preferred zone
                last_failure: None,
                consecutive_failures: 0,
            },
        )
        .await
        .unwrap();

        // Should select node1 despite worse metrics because it's in the preferred zone
        let selected_node = lb.get_next_node().await.unwrap();
        assert_eq!(selected_node, "node1");
    }
}