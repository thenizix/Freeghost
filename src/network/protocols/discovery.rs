// src/network/protocols/discovery.rs
use super::p2p::{P2PNetwork, NetworkPeer, PeerCapabilities};
use crate::utils::error::Result;
use libp2p::{
    kad::{Kademlia, KademliaConfig, KademliaEvent, QueryResult},
    swarm::{NetworkBehaviour, SwarmEvent},
    PeerId, Swarm,
};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::RwLock;

pub struct DiscoveryService {
    network: Arc<P2PNetwork>,
    swarm: Arc<RwLock<Swarm<Kademlia<QueryResult>>>>,
    known_peers: Arc<RwLock<HashSet<PeerId>>>,
    bootstrap_peers: Vec<String>,
    running: Arc<RwLock<bool>>,
}

impl DiscoveryService {
    pub fn new(network: Arc<P2PNetwork>, bootstrap_peers: Vec<String>) -> Self {
        let config = KademliaConfig::default();
        let kademlia = Kademlia::with_config(PeerId::random(), config);
        let swarm = Swarm::new(kademlia);
        
        Self {
            network,
            swarm: Arc::new(RwLock::new(swarm)),
            known_peers: Arc::new(RwLock::new(HashSet::new())),
            bootstrap_peers,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;

        // Start discovery loop
        let running = self.running.clone();
        let swarm = self.swarm.clone();
        let network = self.network.clone();
        let known_peers = self.known_peers.clone();
        
        tokio::spawn(async move {
            while *running.read().await {
                let mut swarm = swarm.write().await;
                
                match swarm.next_event().await {
                    SwarmEvent::Behaviour(KademliaEvent::QueryResult { result, .. }) => {
                        match result {
                            QueryResult::GetClosestPeers(Ok(peers)) => {
                                for peer_id in peers {
                                    if !known_peers.read().await.contains(&peer_id) {
                                        if let Some(addresses) = swarm.addresses_of_peer(&peer_id) {
                                            let peer = NetworkPeer {
                                                id: peer_id.into(),
                                                addresses: addresses.into_iter()
                                                    .map(|a| a.to_string())
                                                    .collect(),
                                                transport_types: vec![
                                                    TransportType::Tcp,
                                                    TransportType::Quic
                                                ],
                                                last_seen: chrono::Utc::now().timestamp(),
                                                capabilities: PeerCapabilities::default(),
                                            };

                                            if let Ok(()) = network.connect_to_peer(peer).await {
                                                known_peers.write().await.insert(peer_id);
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        });

        // Bootstrap with initial peers
        self.bootstrap().await?;
        
        Ok(())
    }

    async fn bootstrap(&self) -> Result<()> {
        let mut swarm = self.swarm.write().await;
        
        for addr in &self.bootstrap_peers {
            if let Ok(peer_id) = addr.parse() {
                swarm.behaviour_mut().add_address(&peer_id, addr.parse()?);
            }
        }
        
        swarm.behaviour_mut().bootstrap()?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        Ok(())
    }
}
