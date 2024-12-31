// src/network/coordinator.rs
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct NetworkCoordinator {
    tor_network: Arc<Mutex<TorNetwork>>,
    p2p_network: Arc<Mutex<P2PNetwork>>,
    quantum_crypto: Arc<QuantumResistantProcessor>,
    active_protocol: NetworkProtocol,
    health_monitor: HealthMonitor,
}

#[derive(Debug, Clone, Copy)]
enum NetworkProtocol {
    Tor,
    P2P,
}

struct HealthMonitor {
    tor_status: Arc<AtomicBool>,
    p2p_status: Arc<AtomicBool>,
    metrics_reporter: MetricsReporter,
}

impl NetworkCoordinator {
    pub async fn new(config: NetworkConfig) -> Result<Self> {
        let quantum_crypto = Arc::new(QuantumResistantProcessor::new());
        
        // Initialize Tor network
        let tor_network = Arc::new(Mutex::new(TorNetwork::new(
            config.tor_config,
            quantum_crypto.clone(),
        )?));

        // Initialize P2P network
        let p2p_network = Arc::new(Mutex::new(P2PNetwork::new(
            quantum_crypto.clone(),
            config.bootstrap_peers,
            &config.storage_path,
        ).await?));

        let health_monitor = HealthMonitor::new();

        Ok(Self {
            tor_network,
            p2p_network,
            quantum_crypto,
            active_protocol: NetworkProtocol::Tor,
            health_monitor,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        // Start both networks
        self.tor_network.lock().await.connect().await?;
        self.p2p_network.lock().await.connect().await?;

        // Start health monitoring
        self.start_health_monitoring().await;

        // Start network coordination
        self.coordinate_networks().await;

        Ok(())
    }

    async fn coordinate_networks(&self) {
        tokio::spawn(async move {
            loop {
                self.check_and_switch_networks().await;
                self.update_metrics().await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    async fn check_and_switch_networks(&self) {
        let tor_healthy = self.health_monitor.tor_status.load(Ordering::Relaxed);
        let p2p_healthy = self.health_monitor.p2p_status.load(Ordering::Relaxed);

        match (tor_healthy, p2p_healthy) {
            (true, _) => {
                if !matches!(self.active_protocol, NetworkProtocol::Tor) {
                    self.switch_to_protocol(NetworkProtocol::Tor).await;
                }
            }
            (false, true) => {
                if !matches!(self.active_protocol, NetworkProtocol::P2P) {
                    self.switch_to_protocol(NetworkProtocol::P2P).await;
                }
            }
            (false, false) => {
                // Both networks are down, attempt recovery
                self.attempt_network_recovery().await;
            }
        }
    }

    async fn switch_to_protocol(&self, protocol: NetworkProtocol) {
        match protocol {
            NetworkProtocol::Tor => {
                if let Ok(mut tor) = self.tor_network.lock().await {
                    if tor.connect().await.is_ok() {
                        self.active_protocol = NetworkProtocol::Tor;
                        metrics::counter!("network.switch.tor", 1);
                    }
                }
            }
            NetworkProtocol::P2P => {
                if let Ok(mut p2p) = self.p2p_network.lock().await {
                    if p2p.connect().await.is_ok() {
                        self.active_protocol = NetworkProtocol::P2P;
                        metrics::counter!("network.switch.p2p", 1);
                    }
                }
            }
        }
    }

    async fn attempt_network_recovery(&self) {
        let recovery_attempts = Arc::new(AtomicU32::new(0));
        
        loop {
            let attempts = recovery_attempts.fetch_add(1, Ordering::Relaxed);
            if attempts >= 3 {
                metrics::counter!("network.recovery.failed", 1);
                break;
            }

            // Try to recover Tor first
            if let Ok(mut tor) = self.tor_network.lock().await {
                if tor.connect().await.is_ok() {
                    metrics::counter!("network.recovery.tor_success", 1);
                    break;
                }
            }

            // Try P2P if Tor fails
            if let Ok(mut p2p) = self.p2p_network.lock().await {
                if p2p.connect().await.is_ok() {
                    metrics::counter!("network.recovery.p2p_success", 1);
                    break;
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    pub async fn send_message(&self, message: SecureMessage) -> Result<()> {
        match self.active_protocol {
            NetworkProtocol::Tor => {
                let tor = self.tor_network.lock().await;
                tor.send_message(message).await
            }
            NetworkProtocol::P2P => {
                let p2p = self.p2p_network.lock().await;
                p2p.send_message(message).await
            }
        }
    }

    pub async fn receive_message(&self) -> Result<SecureMessage> {
        match self.active_protocol {
            NetworkProtocol::Tor => {
                let tor = self.tor_network.lock().await;
                tor.receive_message().await
            }
            NetworkProtocol::P2P => {
                let p2p = self.p2p_network.lock().await;
                p2p.receive_message().await
            }
        }
    }

    async fn update_metrics(&self) {
        metrics::gauge!(
            "network.active_protocol",
            match self.active_protocol {
                NetworkProtocol::Tor => 0.0,
                NetworkProtocol::P2P => 1.0,
            }
        );

        if let Ok(p2p) = self.p2p_network.lock().await {
            let stats = p2p.get_stats().await;
            metrics::gauge!("network.p2p.peers", stats.peer_count as f64);
            metrics::gauge!("network.p2p.bandwidth", stats.bandwidth_usage as f64);
        }

        if let Ok(tor) = self.tor_network.lock().await {
            let stats = tor.get_stats().await;
            metrics::gauge!("network.tor.circuits", stats.circuit_count as f64);
            metrics::gauge!("network.tor.latency", stats.average_latency as f64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_network_coordination() {
        let config = NetworkConfig {
            tor_config: TorConfig::default(),
            bootstrap_peers: vec!["peer1".to_string(), "peer2".to_string()],
            storage_path: "test_storage".to_string(),
        };

        let mut coordinator = NetworkCoordinator::new(config).await.unwrap();
        
        // Test startup
        assert!(coordinator.start().await.is_ok());
        
        // Test message sending
        let message = SecureMessage::new(b"test".to_vec());
        assert!(coordinator.send_message(message).await.is_ok());
        
        // Test protocol switching
        coordinator.switch_to_protocol(NetworkProtocol::P2P).await;
        assert!(matches!(coordinator.active_protocol, NetworkProtocol::P2P));
        
        // Test recovery
        coordinator.attempt_network_recovery().await;
        assert!(coordinator.health_monitor.tor_status.load(Ordering::Relaxed) || 
               coordinator.health_monitor.p2p_status.load(Ordering::Relaxed));
    }
}