// src/network/sync/reconciliation.rs
pub struct StateReconciliation {
    state_manager: Arc<StateManager>,
    network: Arc<P2PNetwork>,
    reconciliation_interval: Duration,
}

impl StateReconciliation {
    pub fn new(
        state_manager: Arc<StateManager>,
        network: Arc<P2PNetwork>,
        reconciliation_interval: Duration,
    ) -> Self {
        Self {
            state_manager,
            network,
            reconciliation_interval,
        }
    }

    pub async fn start_reconciliation(&self) -> Result<()> {
        let state_manager = self.state_manager.clone();
        let network = self.network.clone();
        let interval = self.reconciliation_interval;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                
                let peers = network.get_active_peers().await;
                for peer in peers {
                    if let Err(e) = state_manager.sync_with_peer(peer.id).await {
                        log::error!("Failed to sync with peer {}: {}", peer.id, e);
                    }
                }
            }
        });

        Ok(())
    }
}

// Integration with existing P2PNetwork implementation
impl P2PNetwork {
    pub async fn request_state(&self, peer_id: Uuid) -> Result<NetworkState> {
        let message = NetworkMessage {
            id: Uuid::new_v4(),
            message_type: MessageType::StateRequest,
            payload: vec![],
            timestamp: chrono::Utc::now().timestamp(),
            sender: "self".into(),
            recipient: Some(peer_id.to_string()),
        };

        self.send_to_peer(peer_id, message).await?;
        
        // Wait for response with timeout
        let response = tokio::time::timeout(
            Duration::from_secs(10),
            self.wait_for_state_response(peer_id)
        ).await??;

        Ok(response)
    }

    async fn wait_for_state_response(&self, peer_id: Uuid) -> Result<NetworkState> {
        // Implementation details for waiting for state response
        todo!("Implement state response handling")
    }
}

// Integration with MessageHandler
impl MessageHandler {
    fn register_state_handlers(&mut self) {
        self.register_handler(
            MessageType::StateRequest,
            Box::new(StateRequestProcessor::new(self.state_manager.clone())),
        );
        self.register_handler(
            MessageType::StateResponse,
            Box::new(StateResponseProcessor::new(self.state_manager.clone())),
        );
    }
}

#[async_trait]
impl MessageProcessor for StateRequestProcessor {
    async fn process(&self, message: NetworkMessage) -> Result<()> {
        let current_state = self.state_manager.get_current_state().await?;
        let response = NetworkMessage {
            id: Uuid::new_v4(),
            message_type: MessageType::StateResponse,
            payload: serde_json::to_vec(&current_state)?,
            timestamp: chrono::Utc::now().timestamp(),
            sender: "self".into(),
            recipient: Some(message.sender),
        };

        self.network.send_message(response).await
    }
}