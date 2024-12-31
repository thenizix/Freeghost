// src/network/p2p/mod.rs
use super::{
    transport::{Transport, TransportFactory, TransportType},
    types::{NetworkMessage, NetworkPeer, PeerCapabilities},
    error::{NetworkError, Result},
};
use crate::{
    storage::encrypted::EncryptedStore,
    utils::metrics::NetworkMetrics,
};
use tokio::sync::{RwLock, broadcast, mpsc};
use std::{collections::HashMap, sync::Arc, time::Duration};

pub struct P2PNetwork {
    peers: Arc<RwLock<HashMap<Uuid, NetworkPeer>>>,
    active_connections: Arc<RwLock<HashMap<Uuid, Box<dyn Transport>>>>,
    state_manager: Arc<StateManager>,
    message_handler: Arc<MessageHandler>,
    metrics: Arc<NetworkMetrics>,
    event_tx: broadcast::Sender<NetworkEvent>,
    shutdown_tx: mpsc::Sender<()>,
    config: P2PConfig,
}

#[derive(Clone)]
pub struct P2PConfig {
    pub max_peers: usize,
    pub connection_timeout: Duration,
    pub peer_cleanup_interval: Duration,
    pub heartbeat_interval: Duration,
    pub sync_interval: Duration,
    pub bootstrap_peers: Vec<String>,
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            max_peers: 50,
            connection_timeout: Duration::from_secs(30),
            peer_cleanup_interval: Duration::from_secs(300),
            heartbeat_interval: Duration::from_secs(60),
            sync_interval: Duration::from_secs(600),
            bootstrap_peers: vec![],
        }
    }
}

impl P2PNetwork {
    pub async fn new(
        config: P2PConfig,
        store: Arc<EncryptedStore>,
    ) -> Result<Self> {
        let (event_tx, _) = broadcast::channel(1000);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let metrics = Arc::new(NetworkMetrics::new());

        let state_manager = Arc::new(StateManager::new(store.clone()));
        let message_handler = Arc::new(MessageHandler::new(state_manager.clone()));

        let network = Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            active_connections: Arc::new(RwLock::new(HashMap::new())),
            state_manager,
            message_handler,
            metrics,
            event_tx,
            shutdown_tx,
            config,
        };

        network.start_background_tasks(shutdown_rx).await?;
        Ok(network)
    }

    async fn start_background_tasks(
        &self,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) -> Result<()> {
        let network = Arc::new(self.clone());
        
        // Start maintenance tasks
        let maintenance_task = {
            let network = network.clone();
            tokio::spawn(async move {
                let mut cleanup_interval = tokio::time::interval(network.config.peer_cleanup_interval);
                let mut heartbeat_interval = tokio::time::interval(network.config.heartbeat_interval);
                let mut sync_interval = tokio::time::interval(network.config.sync_interval);

                loop {
                    tokio::select! {
                        _ = cleanup_interval.tick() => {
                            if let Err(e) = network.cleanup_inactive_peers().await {
                                log::error!("Peer cleanup failed: {}", e);
                            }
                        }
                        _ = heartbeat_interval.tick() => {
                            if let Err(e) = network.send_heartbeats().await {
                                log::error!("Heartbeat failed: {}", e);
                            }
                        }
                        _ = sync_interval.tick() => {
                            if let Err(e) = network.sync_network_state().await {
                                log::error!("State sync failed: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            log::info!("Shutting down P2P network background tasks");
                            break;
                        }
                    }
                }
            })
        };

        Ok(())
    }

    pub async fn connect_to_peer(&self, peer: NetworkPeer) -> Result<()> {
        let peer_id = peer.id;
        
        // Check peer limit
        if self.active_connections.read().await.len() >= self.config.max_peers {
            return Err(NetworkError::TooManyPeers);
        }

        // Try each transport type
        for transport_type in peer.transport_types.iter() {
            for address in &peer.addresses {
                match self.establish_connection(*transport_type, address).await {
                    Ok(transport) => {
                        self.active_connections.write().await.insert(peer_id, transport);
                        self.peers.write().await.insert(peer_id, peer.clone());
                        self.metrics.peer_connected();
                        
                        // Trigger state sync with new peer
                        self.sync_with_peer(peer_id).await?;
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
        transport.set_timeout(self.config.connection_timeout).await?;
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
                self.metrics.peer_removed();
            }
        }

        Ok(())
    }

    async fn send_heartbeats(&self) -> Result<()> {
        let heartbeat = NetworkMessage {
            id: Uuid::new_v4(),
            message_type: MessageType::Heartbeat,
            payload: vec![],
            timestamp: chrono::Utc::now().timestamp(),
            sender: "self".into(),
            recipient: None,
        };

        self.broadcast(heartbeat).await
    }

    async fn cleanup_inactive_peers(&self) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let mut peers = self.peers.write().await;
        let mut connections = self.active_connections.write().await;
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
            self.metrics.peers_removed(removed);
        }

        Ok(())
    }

    async fn sync_network_state(&self) -> Result<()> {
        let peers = self.get_active_peers().await;
        for peer in peers {
            if let Err(e) = self.sync_with_peer(peer.id).await {
                log::warn!("Failed to sync with peer {}: {}", peer.id, e);
            }
        }
        Ok(())
    }

    pub async fn get_active_peers(&self) -> Vec<NetworkPeer> {
        self.peers.read().await.values().cloned().collect()
    }

    pub async fn shutdown(&self) -> Result<()> {
        // Signal background tasks to shut down
        let _ = self.shutdown_tx.send(()).await;

        // Disconnect from all peers
        let mut connections = self.active_connections.write().await;
        for (_, transport) in connections.drain() {
            if let Err(e) = transport.disconnect().await {
                log::error!("Error disconnecting transport: {}", e);
            }
        }

        Ok(())
    }
}

// Add tests
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_peer_connection_and_heartbeat() {
        let store = Arc::new(EncryptedStore::new_in_memory().await.unwrap());
        let config = P2PConfig {
            heartbeat_interval: Duration::from_millis(100),
            ..Default::default()
        };

        let network = P2PNetwork::new(config, store).await.unwrap();

        // Create test peer
        let peer = NetworkPeer {
            id: Uuid::new_v4(),
            addresses: vec!["127.0.0.1:8000".to_string()],
            transport_types: vec![TransportType::Tcp],
            last_seen: chrono::Utc::now().timestamp(),
            capabilities: PeerCapabilities::default(),
        };

        // Connect to peer
        network.connect_to_peer(peer.clone()).await.unwrap();

        // Wait for heartbeat
        timeout(Duration::from_millis(200), async {
            loop {
                if network.metrics.heartbeat_sent.load(Ordering::Relaxed) > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        network.shutdown().await.unwrap();
    }
}