// src/network/protocols/tor.rs
use tor_client::{TorClient, TorConfig};

pub struct TorProtocol {
    client: TorClient,
    onion_address: String,
    peers: Vec<String>,
}

#[async_trait]
impl NetworkProtocol for TorProtocol {
    async fn broadcast(&self, message: NetworkMessage) -> Result<()> {
        for peer in &self.peers {
            let response = self.client
                .post(&format!("{}/message", peer))
                .json(&message)
                .send()
                .await?;
                
            if !response.status().is_success() {
                tracing::warn!("Failed to broadcast to peer: {}", peer);
            }
        }
        Ok(())
    }

    async fn verify_template(&self, template: &Template) -> Result<bool> {
        let message = NetworkMessage {
            id: Uuid::new_v4().to_string(),
            message_type: MessageType::TemplateVerification,
            payload: serde_json::to_vec(template)?,
            timestamp: chrono::Utc::now().timestamp(),
        };

        let responses = futures::future::join_all(
            self.peers.iter().map(|peer| {
                self.client
                    .post(&format!("{}/verify", peer))
                    .json(&message)
                    .send()
            })
        ).await;

        let valid_responses = responses
            .into_iter()
            .filter_map(Result::ok)
            .filter(|r| r.status().is_success())
            .count();

        Ok(valid_responses >= self.peers.len() / 2)
    }
}
