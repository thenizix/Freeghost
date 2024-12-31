// src/network/p2p/storage.rs
use rocksdb::{DB, Options};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct StoredPeerData {
    addresses: Vec<String>,
    reputation: f64,
    last_seen: i64,
    bandwidth_usage: BandwidthStats,
}

pub struct PeerStorage {
    db: DB,
}

impl PeerStorage {
    pub fn new(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)
            .map_err(|e| NetworkError::P2PError(format!("Storage error: {}", e)))?;
        
        Ok(Self { db })
    }

    pub async fn store_peer(&self, peer_id: &PeerId, data: &StoredPeerData) -> Result<()> {
        let serialized = bincode::serialize(data)
            .map_err(|e| NetworkError::P2PError(e.to_string()))?;
        
        self.db.put(peer_id.to_bytes(), serialized)
            .map_err(|e| NetworkError::P2PError(e.to_string()))?;
        
        Ok(())
    }

    pub async fn load_peer(&self, peer_id: &PeerId) -> Result<Option<StoredPeerData>> {
        if let Some(data) = self.db.get(peer_id.to_bytes())
            .map_err(|e| NetworkError::P2PError(e.to_string()))? {
            
            let peer_data = bincode::deserialize(&data)
                .map_err(|e| NetworkError::P2PError(e.to_string()))?;
            
            Ok(Some(peer_data))
        } else {
            Ok(None)
        }
    }
}

// src/network/p2p/bandwidth.rs
use std::time::{Duration, Instant};
use metrics::{gauge, counter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthStats {
    bytes_sent: u64,
    bytes_received: u64,
    last_reset: i64,
}

pub struct BandwidthManager {
    limits: BandwidthLimits,
    peer_stats: HashMap<PeerId, BandwidthStats>,
    window_start: Instant,
}

struct BandwidthLimits {
    max_bytes_per_second: u64,
    max_peers: usize,
    reset_interval: Duration,
}

impl BandwidthManager {
    pub fn new(max_bytes_per_second: u64, max_peers: usize) -> Self {
        Self {
            limits: BandwidthLimits {
                max_bytes_per_second,
                max_peers,
                reset_interval: Duration::from_secs(60),
            },
            peer_stats: HashMap::new(),
            window_start: Instant::now(),
        }
    }

    pub fn record_transfer(&mut self, peer_id: &PeerId, bytes: u64, is_incoming: bool) -> bool {
        self.check_window_reset();
        
        let stats = self.peer_stats.entry(*peer_id).or_insert(BandwidthStats {
            bytes_sent: 0,
            bytes_received: 0,
            last_reset: chrono::Utc::now().timestamp(),
        });

        if is_incoming {
            stats.bytes_received += bytes;
            gauge!("bandwidth.received", bytes as f64, "peer_id" => peer_id.to_string());
        } else {
            stats.bytes_sent += bytes;
            gauge!("bandwidth.sent", bytes as f64, "peer_id" => peer_id.to_string());
        }

        let total_bytes = stats.bytes_sent + stats.bytes_received;
        total_bytes <= self.limits.max_bytes_per_second
    }

    fn check_window_reset(&mut self) {
        if self.window_start.elapsed() >= self.limits.reset_interval {
            self.peer_stats.clear();
            self.window_start = Instant::now();
            counter!("bandwidth.window_reset", 1);
        }
    }
}

// src/network/p2p/circuit_breaker.rs
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    last_failure: Option<Instant>,
    reset_timeout: Duration,
    failure_threshold: u32,
}

impl CircuitBreaker {
    pub fn new(reset_timeout: Duration, failure_threshold: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            last_failure: None,
            reset_timeout,
            failure_threshold,
        }
    }

    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                self.state = CircuitState::Closed;
                self.failure_count = 0;
                counter!("circuit_breaker.closed", 1);
            }
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::Open => {}
        }
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());

        if self.failure_count >= self.failure_threshold {
            self.state = CircuitState::Open;
            counter!("circuit_breaker.opened", 1);
        }
    }

    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(last_failure) = self.last_failure {
                    if last_failure.elapsed() >= self.reset_timeout {
                        self.state = CircuitState::HalfOpen;
                        counter!("circuit_breaker.half_open", 1);
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
            CircuitState::HalfOpen => true,
        }
    }
}

// Update P2PNetwork with new components
pub struct P2PNetwork {
    swarm: Mutex<Swarm<P2PBehaviour>>,
    peers: Arc<Mutex<HashMap<PeerId, PeerInfo>>>,
    quantum_crypto: Arc<QuantumResistantProcessor>,
    message_sender: mpsc::Sender<SecureMessage>,
    message_receiver: Mutex<mpsc::Receiver<SecureMessage>>,
    reputation_manager: Arc<Mutex<ReputationManager>>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
    peer_storage: Arc<PeerStorage>,
    bandwidth_manager: Arc<Mutex<BandwidthManager>>,
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
}

impl P2PNetwork {
    pub async fn new(
        quantum_crypto: Arc<QuantumResistantProcessor>,
        bootstrap_peers: Vec<String>,
        storage_path: &str,
    ) -> Result<Self> {
        // Previous initialization code...

        let peer_storage = Arc::new(PeerStorage::new(storage_path)?);
        let bandwidth_manager = Arc::new(Mutex::new(BandwidthManager::new(
            1_000_000, // 1MB/s
            1000,      // max peers
        )));
        let circuit_breaker = Arc::new(Mutex::new(CircuitBreaker::new(
            Duration::from_secs(60),
            5, // 5 failures
        )));

        Ok(Self {
            swarm,
            peers,
            quantum_crypto,
            message_sender: tx,
            message_receiver: Mutex::new(rx),
            reputation_manager,
            rate_limiter,
            peer_storage,
            bandwidth_manager,
            circuit_breaker,
        })
    }

    async fn handle_events(&self) {
        let mut swarm = self.swarm.lock().await;
        
        loop {
            let mut circuit_breaker = self.circuit_breaker.lock().await;
            if !circuit_breaker.can_execute() {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            match swarm.next_event().await {
                SwarmEvent::Behaviour(event) => {
                    match event {
                        libp2p::kad::KademliaEvent::RoutingUpdated { peer, .. } => {
                            let mut peers = self.peers.lock().await;
                            let mut reputation = self.reputation_manager.lock().await;
                            
                            if reputation.is_peer_trusted(&peer) {
                                if let Ok(Some(stored_data)) = self.peer_storage.load_peer(&peer).await {
                                    peers.insert(peer, PeerInfo {
                                        addresses: stored_data.addresses,
                                        last_seen: chrono::Utc::now().timestamp(),
                                        reputation: stored_data.reputation,
                                    });
                                }
                                circuit_breaker.record_success();
                            }
                        }
                        
                        libp2p::gossipsub::Event::Message {
                            propagation_source,
                            message,
                            ..
                        } => {
                            let mut bandwidth = self.bandwidth_manager.lock().await;
                            if !bandwidth.record_transfer(&propagation_source, message.data.len() as u64, true) {
                                circuit_breaker.record_failure();
                                continue;
                            }

                            // Previous message handling code...
                        }
                        
                        _ => {}
                    }
                }
                SwarmEvent::ConnectionClosed { .. } => {
                    circuit_breaker.record_failure();
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    async fn test_peer_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PeerStorage::new(temp_dir.path().to_str().unwrap()).unwrap();
        
        let peer_id = PeerId::random();
        let data = StoredPeerData {
            addresses: vec!["addr1".to_string()],
            reputation: 1.0,
            last_seen: chrono::Utc::now().timestamp(),
            bandwidth_usage: BandwidthStats {
                bytes_sent: 0,
                bytes_received: 0,
                last_reset: chrono::Utc::now().timestamp(),
            },
        };

        assert!(storage.store_peer(&peer_id, &data).await.is_ok());
        assert!(storage.load_peer(&peer_id).await.unwrap().is_some());
    }

    #[test]
    async fn test_bandwidth_manager() {
        let mut manager = BandwidthManager::new(1000, 10);
        let peer_id = PeerId::random();

        assert!(manager.record_transfer(&peer_id, 500, true));
        assert!(manager.record_transfer(&peer_id, 400, false));
        assert!(!manager.record_transfer(&peer_id, 200, true));
    }

    #[test]
    async fn test_circuit_breaker() {
        let mut breaker = CircuitBreaker::new(Duration::from_secs(1), 3);
        
        assert!(breaker.can_execute());
        
        for _ in 0..3 {
            breaker.record_failure();
        }
        
        assert!(!breaker.can_execute());
        
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(breaker.can_execute());
    }
}