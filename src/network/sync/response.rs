// src/network/sync/response.rs
use super::{NetworkMessage, NetworkState, StateManager};
use tokio::sync::oneshot;
use std::collections::HashMap;

pub struct StateResponseHandler {
    pending_responses: RwLock<HashMap<Uuid, oneshot::Sender<NetworkState>>>,
    state_manager: Arc<StateManager>,
}

impl StateResponseHandler {
    pub fn new(state_manager: Arc<StateManager>) -> Self {
        Self {
            pending_responses: RwLock::new(HashMap::new()),
            state_manager,
        }
    }

    pub async fn register_pending(&self, request_id: Uuid) -> oneshot::Receiver<NetworkState> {
        let (tx, rx) = oneshot::channel();
        self.pending_responses.write().await.insert(request_id, tx);
        rx
    }

    pub async fn handle_response(&self, message: NetworkMessage) -> Result<()> {
        let state: NetworkState = serde_json::from_slice(&message.payload)?;
        
        if let Some(tx) = self.pending_responses.write().await.remove(&message.id) {
            let _ = tx.send(state);
        }
        
        Ok(())
    }
}

// Complete the P2PNetwork todo for state response handling
impl P2PNetwork {
    async fn wait_for_state_response(&self, peer_id: Uuid) -> Result<NetworkState> {
        let request_id = Uuid::new_v4();
        let rx = self.state_response_handler.register_pending(request_id).await;
        
        match rx.await {
            Ok(state) => Ok(state),
            Err(_) => Err(NetworkError::ResponseTimeout(peer_id.to_string())),
        }
    }
    
    async fn handle_connection_loss(&self, peer_id: Uuid) -> Result<()> {
        let mut retries = 0;
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_secs(5);

        while retries < MAX_RETRIES {
            match self.reestablish_connection(peer_id).await {
                Ok(()) => {
                    log::info!("Successfully reestablished connection with peer {}", peer_id);
                    return Ok(());
                }
                Err(e) => {
                    log::warn!(
                        "Failed to reestablish connection with peer {} (attempt {}/{}): {}",
                        peer_id, retries + 1, MAX_RETRIES, e
                    );
                    retries += 1;
                    if retries < MAX_RETRIES {
                        tokio::time::sleep(RETRY_DELAY).await;
                    }
                }
            }
        }

        // Remove peer if all retries failed
        self.remove_peer(peer_id).await?;
        Err(NetworkError::PeerUnreachable(peer_id.to_string()))
    }

    async fn reestablish_connection(&self, peer_id: Uuid) -> Result<()> {
        let peer = self.peers.read().await
            .get(&peer_id)
            .cloned()
            .ok_or_else(|| NetworkError::PeerNotFound(peer_id.to_string()))?;

        // Try each transport type in order of preference
        for transport_type in peer.transport_types.iter() {
            for address in &peer.addresses {
                if let Ok(transport) = self.establish_connection(*transport_type, address).await {
                    self.active_connections.write().await.insert(peer_id, transport);
                    return Ok(());
                }
            }
        }

        Err(NetworkError::ConnectionFailed("All transport attempts failed".into()))
    }
}

// Complete state synchronization implementation
impl StateManager {
    pub async fn verify_state_consistency(&self) -> Result<bool> {
        let mut verified = true;
        let current_state = self.current_state.read().await;
        let stored_updates = self.load_stored_updates().await?;
        
        // Verify state can be reconstructed from stored updates
        let mut reconstructed_state = NetworkState::default();
        for update in stored_updates {
            if let Err(e) = reconstructed_state.apply_update(&update) {
                log::error!("State inconsistency detected: {}", e);
                verified = false;
                break;
            }
        }

        if verified {
            verified = reconstructed_state.hash() == current_state.hash();
        }

        if !verified {
            log::warn!("State verification failed, initiating recovery");
            self.initiate_state_recovery().await?;
        }

        Ok(verified)
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

        // Sort updates by timestamp
        updates.sort_by_key(|u| u.timestamp);
        Ok(updates)
    }

    async fn initiate_state_recovery(&self) -> Result<()> {
        // Get majority state from peers
        let peer_states = self.collect_peer_states().await?;
        let majority_state = self.determine_majority_state(peer_states)?;
        
        // Backup current state before recovery
        self.backup_current_state().await?;
        
        // Apply majority state
        *self.current_state.write().await = majority_state;
        
        Ok(())
    }

    async fn collect_peer_states(&self) -> Result<Vec<NetworkState>> {
        let mut states = Vec::new();
        let peers = self.network.get_active_peers().await;
        
        for peer in peers {
            if let Ok(state) = self.network.request_state(peer.id).await {
                states.push(state);
            }
        }
        
        Ok(states)
    }

    fn determine_majority_state(&self, states: Vec<NetworkState>) -> Result<NetworkState> {
        use std::collections::HashMap;
        let mut state_counts: HashMap<String, (NetworkState, usize)> = HashMap::new();
        
        // Count occurrences of each state hash
        for state in states {
            let hash = state.hash();
            state_counts
                .entry(hash)
                .and_modify(|(_, count)| *count += 1)
                .or_insert((state, 1));
        }

        // Find state with highest count
        state_counts
            .into_iter()
            .max_by_key(|(_, (_, count))| *count)
            .map(|(_, (state, _))| state)
            .ok_or_else(|| NetworkError::NoMajorityState)
    }
}

// Enhanced message processor with resilience
impl MessageHandler {
    pub async fn process_message_with_resilience(&self, message: NetworkMessage) -> Result<()> {
        let processor = self.handlers.get(&message.message_type)
            .ok_or_else(|| NetworkError::NoHandlerFound(message.message_type.to_string()))?;

        let result = tokio::time::timeout(
            Duration::from_secs(30),
            processor.process(message.clone())
        ).await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                log::error!("Error processing message: {}", e);
                self.handle_processing_error(message, e).await
            },
            Err(_) => {
                log::error!("Message processing timeout");
                self.handle_processing_timeout(message).await
            }
        }
    }

    async fn handle_processing_error(&self, message: NetworkMessage, error: NetworkError) -> Result<()> {
        // Implement error-specific recovery strategies
        match error {
            NetworkError::TemporaryFailure(_) => {
                // Retry with backoff
                self.retry_with_backoff(message).await
            },
            NetworkError::StateInconsistency => {
                // Trigger state recovery
                self.state_manager.initiate_state_recovery().await
            },
            _ => Err(error)
        }
    }

    async fn retry_with_backoff(&self, message: NetworkMessage) -> Result<()> {
        let mut delay = Duration::from_millis(100);
        const MAX_RETRIES: u32 = 3;

        for attempt in 0..MAX_RETRIES {
            tokio::time::sleep(delay).await;
            
            match self.process_message_with_resilience(message.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    log::warn!("Retry attempt {}/{} failed: {}", attempt + 1, MAX_RETRIES, e);
                    delay *= 2;
                }
            }
        }

        Err(NetworkError::MaxRetriesExceeded)
    }
}