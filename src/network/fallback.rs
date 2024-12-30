// src/network/fallback.rs
pub struct FallbackProtocol {
    protocols: Vec<Box<dyn NetworkProtocol>>,
}

#[async_trait::async_trait]
pub trait NetworkProtocol: Send + Sync {
    async fn send(&self, message: &NetworkMessage) -> Result<()>;
    async fn receive(&self) -> Result<NetworkMessage>;
}

impl FallbackProtocol {
    pub async fn send_with_fallback(&self, message: NetworkMessage) -> Result<()> {
        for protocol in &self.protocols {
            if let Ok(_) = protocol.send(&message).await {
                return Ok(());
            }
        }
        Err(NodeError::Network("All protocols failed".into()))
    }
}