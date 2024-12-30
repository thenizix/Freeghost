// src/network/protocols/fallback.rs
pub struct FallbackProtocol {
    protocols: Vec<Box<dyn NetworkProtocol>>,
}

impl FallbackProtocol {
    pub async fn broadcast_with_retry(&self, message: NetworkMessage) -> Result<()> {
        let mut last_error = None;
        
        for protocol in &self.protocols {
            match protocol.broadcast(message.clone()).await {
                Ok(_) => return Ok(()),
                Err(e) => last_error = Some(e),
            }
        }
        
        Err(last_error.unwrap_or_else(|| 
            NodeError::Network("All protocols failed".into())))
    }
}