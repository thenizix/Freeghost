// src/network/state/mod.rs
use super::{
    types::{NetworkState, StateUpdate, SyncStatus},
    error::{NetworkError, Result},
};
use crate::storage::encrypted::EncryptedStore;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

pub struct StateManager {
    store: Arc<EncryptedStore>,
    current_state: RwLock<NetworkState>,
    sync_status: RwLock<HashMap<Uuid, SyncStatus>>,
    update_channel: broadcast::Sender<StateUpdate>,
}

impl StateManager {
    pub fn new(store: Arc<EncryptedStore>) -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            store,
            current_state: RwLock::new(NetworkState::default()),
            sync_status: RwLock::new(HashMap::new()),
            update_channel: tx,
        }
    }

    pub async fn apply_update(&self, update: StateUpdate) -> Result<()> {
        // Verify update
        update.verify_signature()?;

        // Apply to current state
        {
            let mut state = self.current_state.write().await;
            state.apply_update(&update)?;
        }

        // Store update
        self.store.store(
            format!("state_update_{}", update.id).as_bytes(),
            &update,
        ).await?;

        // Broadcast update
        let _ = self.update_channel.send(update);

        Ok(())
    }

    pub async fn get_current_state(&self) -> Result<NetworkState> {
        Ok(self.current_state.read().await.clone())
    }

    pub async fn get_state_diff(&self, peer_id: Uuid) -> Result<Vec<StateUpdate>> {
        let status = self.sync_status.read().await
            .get(&peer_id)
            .cloned()
            .unwrap_or_default();

        let current_state = self.current_state.read().await;
        current_state.get_updates_since(status.last_update)
    }

    pub async fn verify_state_consistency(&self) -> Result<bool> {
        let current_state = self.current_state.read().await;
        let stored_updates = self.load_stored_updates().await?;

        // Reconstruct state from updates
        let mut reconstructed_state = NetworkState::default();
        for update in stored_updates {
            if let Err(e) = reconstructed_state.apply_update(&update) {
                log::error!("State consistency error: {}", e);
                return Ok(false);
            }
        }

        Ok(reconstructed_state.hash() == current_state.hash())
    }

    async fn load_stored_updates(&self) -> Result<Vec<StateUpdate>> {
        let mut updates = Vec::new();
        let prefix = "state_update_".as_bytes();
        
        let stored_keys = self.store.get_keys_with_prefix(prefix).await?;
        for key in stored_keys {
            if let Some(update) = self.store.retrieve::<StateUpdate>(&key).await? {
                updates.push(update);
            }
        }

        updates.sort_by_key(|u| u.timestamp);
        Ok(updates)
    }

    pub async fn handle_state_recovery(&self) -> Result<()> {
        // Backup current state
        self.backup_current_state().await?;

        // Get majority state from peers
        let majority_state = self.get_majority_state().await?;

        // Apply majority state
        *self.current_state.write().await = majority_state;

        Ok(())
    }

    async fn backup_current_state(&self) -> Result<()> {
        let state = self.current_state.read().await;
        let backup_key = format!("state_backup_{}", chrono::Utc::now().timestamp());
        self.store.store(backup_key.as_bytes(), &*state).await?;
        Ok(())
    }

    async fn get_majority_state(&self) -> Result<NetworkState> {
        let peer_states = self.collect_peer_states().await?;
        
        let mut state_counts: HashMap<String, (NetworkState, usize)> = HashMap::new();
        for state in peer_states {
            let hash = state.hash();
            state_counts
                .entry(hash)
                .and_modify(|(_, count)| *count += 1)
                .or_insert((state, 1));
        }

        state_counts
            .into_iter()
            .max_by_key(|(_, (_, count))| *count)
            .map(|(_, (state, _))| state)
            .ok_or_else(|| NetworkError::NoMajorityState)
    }
}

// Add tests
#[cfg(test)]
mod tests {
    use super::*;