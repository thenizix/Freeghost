// src/network/transport/tcp.rs
use super::{Transport, TransportType, NetworkMessage, Result, TransportError};
use tokio::{
    net::TcpStream,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::RwLock,
};
use std::{sync::Arc, time::Duration};

pub struct TcpTransport {
    stream: Arc<RwLock<Option<TcpStream>>>,
    address: String,
    timeout: Duration,
    metrics: Arc<NetworkMetrics>,
}

impl TcpTransport {
    pub fn new(address: String, metrics: Arc<NetworkMetrics>) -> Self {
        Self {
            stream: Arc::new(RwLock::new(None)),
            address,
            timeout: Duration::from_secs(30),
            metrics,
        }
    }

    async fn ensure_connected(&self) -> Result<()> {
        if !self.is_connected().await {
            return Err(TransportError::Unavailable("Not connected".to_string()));
        }
        Ok(())
    }
}

#[async_trait]
impl Transport for TcpTransport {
    fn transport_type(&self) -> TransportType {
        TransportType::Tcp
    }

    async fn connect(&mut self) -> Result<()> {
        let stream = TcpStream::connect(&self.address)
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;

        stream.set_nodelay(true)?;
        stream.set_keepalive(Some(Duration::from_secs(60)))?;

        *self.stream.write().await = Some(stream);
        self.metrics.connection_established();
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(stream) = self.stream.write().await.take() {
            stream.shutdown().await?;
            self.metrics.connection_closed();
        }
        Ok(())
    }

    async fn send(&self, message: NetworkMessage) -> Result<()> {
        self.ensure_connected().await?;
        let stream_lock = self.stream.read().await;
        let stream = stream_lock.as_ref().unwrap();

        let serialized = serde_json::to_vec(&message)
            .map_err(|e| TransportError::InvalidMessage(e.to_string()))?;

        let len = serialized.len() as u32;
        let len_bytes = len.to_be_bytes();

        let mut buffer = Vec::with_capacity(4 + serialized.len());
        buffer.extend_from_slice(&len_bytes);
        buffer.extend_from_slice(&serialized);

        stream.write_all(&buffer).await?;
        stream.flush().await?;

        self.metrics.message_sent();
        Ok(())
    }

    async fn receive(&self) -> Result<NetworkMessage> {
        self.ensure_connected().await?;
        let stream_lock = self.stream.read().await;
        let stream = stream_lock.as_ref().unwrap();

        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        if len > 16 * 1024 * 1024 { // 16MB limit
            return Err(TransportError::InvalidMessage("Message too large".to_string()));
        }

        let mut buffer = vec![0u8; len];
        stream.read_exact(&mut buffer).await?;

        let message = serde_json::from_slice(&buffer)
            .map_err(|e| TransportError::InvalidMessage(e.to_string()))?;

        self.metrics.message_received();
        Ok(message)
    }

    async fn is_connected(&self) -> bool {
        self.stream.read().await.is_some()
    }

    async fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.timeout = timeout;
        if let Some(stream) = self.stream.write().await.as_mut() {
            stream.set_read_timeout(Some(timeout))?;
            stream.set_write_timeout(Some(timeout))?;
        }
        Ok(())
    }
}
