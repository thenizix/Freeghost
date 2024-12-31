// src/network/sync/state.rs
use super::types::{NetworkState, StateUpdate, SyncStatus};
use crate::storage::encrypted::EncryptedStore;
use std::collections::HashMap;

pub struct StateManager {
    store: Arc<EncryptedStore>,
    current_state: RwLock<NetworkState>,
    sync_status: RwLock<HashMap<Uuid, SyncStatus>>,
    update_channel: broadcast::Sender<StateUpdate>,
}

impl StateManager {
    pub fn new(store: Arc<EncryptedStore>) -> (Self, broadcast::Receiver<StateUpdate>) {
        let (tx, rx) = broadcast::channel(1000);
        
        (Self {
            store,
            current_state: RwLock::new(NetworkState::default()),
            sync_status: RwLock::new(HashMap::new()),
            update_channel: tx,
        }, rx)
    }

    pub async fn apply_update(&self, update: StateUpdate) -> Result<()> {
        // Verify update signature
        update.verify_signature()?;

        // Apply update to current state
        let mut state = self.current_state.write().await;
        state.apply_update(&update)?;

        // Store update
        self.store.store(
            format!("state_update_{}", update.id).as_bytes(),
            &update,
        ).await?;

        // Broadcast update to subscribers
        let _ = self.update_channel.send(update);

        Ok(())
    }

    pub async fn get_state_diff(&self, peer_id: Uuid) -> Result<Vec<StateUpdate>> {
        let status = self.sync_status.read().await
            .get(&peer_id)
            .cloned()
            .unwrap_or_default();

        let current_state = self.current_state.read().await;
        current_state.get_updates_since(status.last_update)
    }

    pub async fn sync_with_peer(&self, peer_id: Uuid) -> Result<()> {
        // Get peer's current state
        let peer_state = self.network
            .request_state(peer_id)
            .await?;

        // Compare and get missing updates
        let diff = self.get_state_diff(peer_id).await?;
        
        // Apply missing updates
        for update in diff {
            self.apply_update(update).await?;
        }

        // Update sync status
        self.sync_status.write().await
            .insert(peer_id, SyncStatus {
                last_update: chrono::Utc::now().timestamp(),
                state_hash: peer_state.hash(),
            });

        Ok(())
    }
}
