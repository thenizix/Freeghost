use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};
use crate::utils::error::Result;
use crate::network::p2p::P2PNetwork;

pub struct LoadBalancer {
    nodes: Arc<RwLock<HashMap<String, NodeMetrics>>>,
}

#[derive(Debug, Clone)]
pub struct NodeMetrics {
    pub node_id: String,
    pub latency: u64,
    pub error_rate: f64,
    pub load: u32,
}

impl LoadBalancer {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_node(&self, node_id: String, metrics: NodeMetrics) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        nodes.insert(node_id, metrics);
        Ok(())
    }

    pub async fn select_node(&self) -> Result<Option<String>> {
        let nodes = self.nodes.read().await;
        let best_node = nodes.values().min_by_key(|metrics| metrics.latency);
        Ok(best_node.map(|metrics| metrics.node_id.clone()))
    }

    pub async fn update_metrics(&self, node_id: &str, latency: u64, error_rate: f64, load: u32) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        if let Some(metrics) = nodes.get_mut(node_id) {
            metrics.latency = latency;
            metrics.error_rate = error_rate;
            metrics.load = load;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_balancer() {
        let load_balancer = LoadBalancer::new();
        load_balancer.add_node("node1".to_string(), NodeMetrics {
            node_id: "node1".to_string(),
            latency: 50,
            error_rate: 0.01,
            load: 10,
        }).await.unwrap();

        // Test selecting the best node
        let best_node = load_balancer.select_node().await.unwrap();
        assert_eq!(best_node, Some("node1".to_string()));

        // Test updating metrics
        load_balancer.update_metrics("node1", 30, 0.02, 20).await.unwrap();
        let best_node = load_balancer.select_node().await.unwrap();
        assert_eq!(best_node, Some("node1".to_string()));
    }
}
