// src/network/p2p.rs
use super::{
    transport::{Transport, TransportFactory, TransportType},
    types::{NetworkPeer, PeerCapabilities},
};
use tokio::sync::{RwLock, broadcast};
use std::{collections::HashMap, sync::Arc, time::Duration};
use crate::utils::metrics::NetworkMetrics;

pub struct P2PNetwork {
    peers: Arc<RwLock<HashMap<Uuid, NetworkPeer>>>,
    active_connections: Arc<RwLock<HashMap<Uuid, Box<dyn Transport>>>>,
    metrics: Arc<NetworkMetrics>,
    event_tx: broadcast::Sender<NetworkEvent>,
    config: P2PConfig,
}

pub struct P2PConfig {
    max_peers: usize,
    connection_timeout: Duration,
    peer_cleanup_interval: Duration,
    heartbeat_interval: Duration,
}

impl P2PNetwork {
    pub async fn new(config: P2PConfig) -> Result<Self> {
        let (event_tx, _) = broadcast::channel(1000);
        
        Ok(Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            active_connections: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(NetworkMetrics::new()),
            event_tx,
            config,
        })
    }

    pub async fn connect_to_peer(&self, peer: NetworkPeer) -> Result<()> {
        let peer_id = peer.id;
        
        // Check if we already have too many peers
        if self.active_connections.read().await.len() >= self.config.max_peers {
            return Err(NetworkError::TooManyPeers);
        }
        
        // Try each transport type in order of preference
        for transport_type in peer.transport_types.iter() {
            for address in &peer.addresses {
                match self.establish_connection(*transport_type, address).await {
                    Ok(transport) => {
                        self.active_connections.write().await.insert(peer_id, transport);
                        self.peers.write().await.insert(peer_id, peer.clone());
                        self.metrics.connection_established();
                        return Ok(());
                    }
                    Err(e) => {
                        log::warn!("Failed to connect to peer {} via {}: {}", peer_id, address, e);
                        continue;
                    }
                }
            }
        }
        
        Err(NetworkError::ConnectionFailed("All transport attempts failed".into()))
    }

    async fn establish_connection(
        &self,
        transport_type: TransportType,
        address: &str,
    ) -> Result<Box<dyn Transport>> {
        let mut transport = TransportFactory::create(transport_type, address)?;
        
        // Set timeout from config
        transport.set_timeout(self.config.connection_timeout).await?;
        
        // Attempt connection
        transport.connect().await?;
        
        Ok(transport)
    }

    pub async fn broadcast(&self, message: NetworkMessage) -> Result<()> {
        let connections = self.active_connections.read().await;
        let mut failed_peers = Vec::new();
        
        for (peer_id, transport) in connections.iter() {
            if let Err(e) = transport.send(message.clone()).await {
                log::error!("Failed to send to peer {}: {}", peer_id, e);
                failed_peers.push(*peer_id);
                self.metrics.message_failed();
            } else {
                self.metrics.message_sent();
            }
        }
        
        // Clean up failed connections
        if !failed_peers.is_empty() {
            let mut connections = self.active_connections.write().await;
            for peer_id in failed_peers {
                connections.remove(&peer_id);
            }
        }
        
        Ok(())
    }

    pub async fn start_network_maintenance(&self) {
        let cleanup_interval = self.config.peer_cleanup_interval;
        let heartbeat_interval = self.config.heartbeat_interval;
        let peers = self.peers.clone();
        let connections = self.active_connections.clone();
        let metrics = self.metrics.clone();
        
        // Start peer cleanup task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            loop {
                interval.tick().await;
                Self::cleanup_inactive_peers(&peers, &connections, &metrics).await;
            }
        });
        
        // Start heartbeat task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);
            loop {
                interval.tick().await;
                Self::send_heartbeats(&connections, &metrics).await;
            }
        });
    }

    async fn cleanup_inactive_peers(
        peers: &RwLock<HashMap<Uuid, NetworkPeer>>,
        connections: &RwLock<HashMap<Uuid, Box<dyn Transport>>>,
        metrics: &NetworkMetrics,
    ) {
        let now = chrono::Utc::now().timestamp();
        let mut peers = peers.write().await;
        let mut connections = connections.write().await;
        let mut removed = 0;
        
        peers.retain(|peer_id, peer| {
            let is_active = now - peer.last_seen < 300; // 5 minutes timeout
            if !is_active {
                connections.remove(peer_id);
                removed += 1;
            }
            is_active
        });
        
        if removed > 0 {
            metrics.peers_removed(removed);
        }
    }

    async fn send_heartbeats(
        connections: &RwLock<HashMap<Uuid, Box<dyn Transport>>>,
        metrics: &NetworkMetrics,
    ) {
        let connections = connections.read().await;
        let heartbeat = NetworkMessage {
            id: Uuid::new_v4(),
            message_type: MessageType::Heartbeat,
            payload: vec![],
            timestamp: chrono::Utc::now().timestamp(),
            sender: "self".into(),
            recipient: None,
        };
        
        for (peer_id, transport) in connections.iter() {
            if let Err(e) = transport.send(heartbeat.clone()).await {
                log::warn!("Heartbeat failed for peer {}: {}", peer_id, e);
                metrics.heartbeat_failed();
            } else {
                metrics.heartbeat_sent();
            }
        }
    }
}