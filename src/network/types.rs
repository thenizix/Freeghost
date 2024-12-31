// src/network/types.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    IdentityVerification,
    TemplateUpdate,
    KeyRotation,
    HealthCheck,
    StateRequest,
    StateResponse,
    Heartbeat,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    pub id: Uuid,
    pub message_type: MessageType,
    pub payload: Vec<u8>,
    pub timestamp: i64,
    pub sender: String,
    pub recipient: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPeer {
    pub id: Uuid,
    pub addresses: Vec<String>,
    pub transport_types: Vec<TransportType>,
    pub last_seen: i64,
    pub capabilities: PeerCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub supports_tor: bool,
    pub supports_quic: bool,
    pub is_validator: bool,
    pub storage_capacity: u64,
    pub bandwidth_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkState {
    pub version: u64,
    pub peers: Vec<NetworkPeer>,
    pub last_updated: i64,
    pub state_hash: String,
}

impl NetworkState {
    pub fn hash(&self) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_vec(self).unwrap_or_default());
        format!("{:x}", hasher.finalize())
    }

    pub fn apply_update(&mut self, update: &StateUpdate) -> Result<()> {
        if update.base_version != self.version {
            return Err(NetworkError::InvalidStateVersion);
        }
        
        // Apply changes
        for change in &update.changes {
            match change {
                StateChange::AddPeer(peer) => {
                    self.peers.push(peer.clone());
                }
                StateChange::RemovePeer(peer_id) => {
                    self.peers.retain(|p| p.id != *peer_id);
                }
                StateChange::UpdatePeer(peer) => {
                    if let Some(existing) = self.peers.iter_mut().find(|p| p.id == peer.id) {
                        *existing = peer.clone();
                    }
                }
            }
        }

        self.version += 1;
        self.last_updated = chrono::Utc::now().timestamp();
        self.state_hash = self.hash();
        
        Ok(())
    }

    pub fn get_updates_since(&self, version: u64) -> Result<Vec<StateUpdate>> {
        if version > self.version {
            return Err(NetworkError::InvalidStateVersion);
        }

        // In a real implementation, this would retrieve updates from storage
        // For now, we'll return a single update to bring them to current version
        let update = StateUpdate {
            id: Uuid::new_v4(),
            base_version: version,
            changes: self.peers.iter()
                .map(|p| StateChange::AddPeer(p.clone()))
                .collect(),
            timestamp: self.last_updated,
            signature: vec![], // Would be properly signed in production
        };

        Ok(vec![update])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateUpdate {
    pub id: Uuid,
    pub base_version: u64,
    pub changes: Vec<StateChange>,
    pub timestamp: i64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateChange {
    AddPeer(NetworkPeer),
    RemovePeer(Uuid),
    UpdatePeer(NetworkPeer),
}

impl StateUpdate {
    pub fn verify_signature(&self) -> Result<()> {
        // In a real implementation, this would verify the cryptographic signature
        // For now, we'll assume all updates are valid
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub last_update: i64,
    pub state_hash: String,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            last_update: 0,
            state_hash: String::new(),
        }
    }
}

impl Default for NetworkState {
    fn default() -> Self {
        Self {
            version: 0,
            peers: Vec::new(),
            last_updated: chrono::Utc::now().timestamp(),
            state_hash: String::new(),
        }
    }
}
