// src/network/transport/quic.rs
use super::{Transport, TransportType, NetworkMessage, Result, TransportError};
use quinn::{Endpoint, Connection, TransportConfig, ClientConfig};
use std::{sync::Arc, time::Duration, net::SocketAddr};
use tokio::sync::RwLock;

pub struct QuicTransport {
    endpoint: Option<Endpoint>,
    connection: Arc<RwLock<Option<Connection>>>,
    address: SocketAddr,
    timeout: Duration,
    metrics: Arc<NetworkMetrics>,
}

impl QuicTransport {
    pub fn new(address: SocketAddr, metrics: Arc<NetworkMetrics>) -> Self {
        Self {
            endpoint: None,
            connection: Arc::new(RwLock::new(None)),
            address,
            timeout: Duration::from_secs(30),
            metrics,
        }
    }

    fn create_client_config() -> ClientConfig {
        let mut transport_config = TransportConfig::default();
        transport_config.keep_alive_interval(Some(Duration::from_secs(15)));
        transport_config.max_idle_timeout(Some(Duration::from_secs(30)));

        let crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        let mut client_config = ClientConfig::new(Arc::new(crypto));
        client_config.transport_config(Arc::new(transport_config));
        client_config
    }
}

#[async_trait]  // Only need this once before the impl block
impl Transport for QuicTransport {    
    fn transport_type(&self) -> TransportType {
        TransportType::Quic
    }
    
    async fn connect(&mut self) -> Result<()> {
        let client_config = Self::create_client_config();
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
        endpoint.set_default_client_config(client_config);
        
        let connection = endpoint
            .connect(self.address, "localhost")?
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        
        self.endpoint = Some(endpoint);
        *self.connection.write().await = Some(connection);
        self.metrics.connection_established();
        Ok(())
    }
    
        async fn disconnect(&mut self) -> Result<()> {
            if let Some(connection) = self.connection.write().await.take() {
                connection.close(0u32.into(), b"closed");
                self.metrics.connection_closed();
            }
            if let Some(endpoint) = self.endpoint.take() {
                endpoint.close(0u32.into(), b"closed");
            }
            Ok(())
        }
    
        async fn send(&self, message: NetworkMessage) -> Result<()> {
            let connection = self.connection.read().await;
            let connection = connection.as_ref()
                .ok_or_else(|| TransportError::Unavailable("Not connected".to_string()))?;
    
            let mut send_stream = connection.open_uni()
                .await
                .map_err(|e| TransportError::SendError(e.to_string()))?;
    
            let serialized = serde_json::to_vec(&message)
                .map_err(|e| TransportError::InvalidMessage(e.to_string()))?;
    
            send_stream.write_all(&(serialized.len() as u32).to_be_bytes()).await?;
            send_stream.write_all(&serialized).await?;
            send_stream.finish().await?;
    
            self.metrics.message_sent();
            Ok(())
        }
    
        async fn receive(&self) -> Result<NetworkMessage> {
            let connection = self.connection.read().await;
            let connection = connection.as_ref()
                .ok_or_else(|| TransportError::Unavailable("Not connected".to_string()))?;
    
            let mut recv_stream = connection.accept_uni()
                .await
                .map_err(|e| TransportError::ReceiveError(e.to_string()))?;
    
            let mut len_bytes = [0u8; 4];
            recv_stream.read_exact(&mut len_bytes).await?;
            let len = u32::from_be_bytes(len_bytes) as usize;
    
            if len > 16 * 1024 * 1024 { // 16MB limit
                return Err(TransportError::InvalidMessage("Message too large".to_string()));
            }
    
            let mut buffer = vec![0u8; len];
            recv_stream.read_exact(&mut buffer).await?;
    
            let message = serde_json::from_slice(&buffer)
                .map_err(|e| TransportError::InvalidMessage(e.to_string()))?;
    
            self.metrics.message_received();
            Ok(message)
        }
    
        async fn is_connected(&self) -> bool {
            self.connection.read().await.is_some()
        }
    
        async fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
            self.timeout = timeout;
            if let Some(conn) = self.connection.read().await.as_ref() {
                // QUIC connection timeouts are handled by the transport config
                conn.set_max_idle_timeout(Some(timeout));
            }
            Ok(())
        }
    }
    
    // Certificate verification for development/testing
    struct SkipServerVerification;
    
    impl rustls::client::ServerCertVerifier for SkipServerVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &rustls::Certificate,
            _intermediates: &[rustls::Certificate],
            _server_name: &rustls::ServerName,
            _scts: &mut dyn Iterator<Item = &[u8]>,
            _ocsp_response: &[u8],
            _now: std::time::SystemTime,
        ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::ServerCertVerified::assertion())
        }
    }
    