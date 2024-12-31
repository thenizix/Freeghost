// src/network/transport/tor.rs
use super::{Transport, TransportType, NetworkMessage, Result, TransportError};
use tokio::sync::RwLock;
use std::{sync::Arc, time::Duration};
use tor_client::{TorClient, TorClientConfig};

pub struct TorTransport {
    client: Arc<RwLock<TorClient>>,
    address: String,
    timeout: Duration,
    metrics: Arc<NetworkMetrics>,
    connected: bool,
}

impl TorTransport {
    pub fn new(address: String, metrics: Arc<NetworkMetrics>) -> Self {
        let config = TorClientConfig::default();
        Self {
            client: Arc::new(RwLock::new(TorClient::new(config))),
            address,
            timeout: Duration::from_secs(60),
            metrics,
            connected: false,
        }
    }
}

#[async_trait]
impl Transport for TorTransport {
    fn transport_type(&self) -> TransportType {
        TransportType::Tor
    }

    async fn connect(&mut self) -> Result<()> {
        let mut client = self.client.write().await;
        client.connect(&self.address)
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        
        self.connected = true;
        self.metrics.connection_established();
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        let mut client = self.client.write().await;
        client.disconnect()
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        
        self.connected = false;
        self.metrics.connection_closed();
        Ok(())
    }

    async fn send(&self, message: NetworkMessage) -> Result<()> {
        if !self.connected {
            return Err(TransportError::Unavailable("Not connected".to_string()));
        }

        let client = self.client.read().await;
        let serialized = serde_json::to_vec(&message)
            .map_err(|e| TransportError::InvalidMessage(e.to_string()))?;

        client.send(&serialized)
            .await
            .map_err(|e| TransportError::SendError(e.to_string()))?;

        self.metrics.message_sent();
        Ok(())
    }

    async fn receive(&self) -> Result<NetworkMessage> {
        if !self.connected {
            return Err(TransportError::Unavailable("Not connected".to_string()));
        }

        let client = self.client.read().await;
        let data = client.receive()
            .await
            .map_err(|e| TransportError::ReceiveError(e.to_string()))?;

        let message = serde_json::from_slice(&data)
            .map_err(|e| TransportError::InvalidMessage(e.to_string()))?;

        self.metrics.message_received();
        Ok(message)
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }

    async fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.timeout = timeout;
        let mut client = self.client.write().await;
        client.set_timeout(timeout)
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        Ok(())
    }
}