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