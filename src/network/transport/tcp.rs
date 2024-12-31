// src/network/transport/tcp.rs

use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::types::{
    Transport, TransportConfig, TransportError, TransportListener,
    TransportMetrics, TransportStream, TcpConfig
};
use crate::core::crypto::types::SecurityLevel;
use crate::network::circuit_breaker::CircuitBreaker;
use crate::utils::error::Result;

pub struct TcpTransport {
    config: TransportConfig,
    tcp_config: TcpConfig,
    circuit_breaker: Option<CircuitBreaker>,
    metrics: Arc<RwLock<TransportMetrics>>,
    // Preserve existing encryption_manager from codebase
    encryption_manager: Arc<EncryptionManager>,
}

impl TcpTransport {
    pub fn new(
        config: TransportConfig,
        tcp_config: TcpConfig,
        circuit_breaker: Option<CircuitBreaker>,
        encryption_manager: Arc<EncryptionManager>,
    ) -> Self {
        Self {
            config,
            tcp_config,
            circuit_breaker,
            metrics: Arc::new(RwLock::new(TransportMetrics {
                latency: Duration::from_secs(0),
                bytes_sent: 0,
                bytes_received: 0,
                active_connections: 0,
                error_count: 0,
            })),
            encryption_manager,
        }
    }

    // Preserve existing helper methods
    async fn update_metrics(&self, new_metrics: TransportMetrics) {
        let mut metrics = self.metrics.write().await;
        *metrics = new_metrics;
    }
}

#[async_trait::async_trait]
impl Transport for TcpTransport {
    async fn connect(&self, address: &str) -> Result<Box<dyn TransportStream>> {
        // Check circuit breaker first
        if let Some(cb) = &self.circuit_breaker {
            if cb.is_open().await {
                return Err(TransportError::CircuitBreakerOpen.into());
            }
        }

        let start = std::time::Instant::now();

        // Use existing connection logic from codebase
        let stream = match tokio::time::timeout(
            self.config.connect_timeout,
            TcpStream::connect(address),
        ).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(e)) => {
                self.record_error().await;
                return Err(TransportError::ConnectionFailed(e.to_string()).into());
            }
            Err(_) => {
                self.record_error().await;
                return Err(TransportError::Timeout.into());
            }
        };

        // Apply TCP-specific configurations
        stream.set_nodelay(self.tcp_config.nodelay)?;
        if let Some(keepalive) = self.tcp_config.keepalive {
            stream.set_keepalive(Some(keepalive))?;
        }
        stream.set_send_buffer_size(self.tcp_config.send_buffer_size)?;
        stream.set_recv_buffer_size(self.tcp_config.recv_buffer_size)?;

        // Update metrics
        let latency = start.elapsed();
        self.update_connection_metrics(latency).await;

        // Wrap stream with existing encryption if needed
        let encrypted_stream = if self.config.security_level != SecurityLevel::None {
            self.encryption_manager.encrypt_stream(stream).await?
        } else {
            stream
        };

        Ok(Box::new(TcpTransportStream {
            stream: encrypted_stream,
            config: self.config.clone(),
        }))
    }

    async fn listen(&self, address: &str) -> Result<Box<dyn TransportListener>> {
        let listener = TcpListener::bind(address).await?;
        
        Ok(Box::new(TcpTransportListener {
            listener,
            config: self.config.clone(),
            encryption_manager: self.encryption_manager.clone(),
        }))
    }

    fn get_config(&self) -> &TransportConfig {
        &self.config
    }

    async fn get_metrics(&self) -> Result<TransportMetrics> {
        Ok(self.metrics.read().await.clone())
    }

    // Implement existing security operations
    async fn rotate_keys(&self) -> Result<()> {
        self.encryption_manager.rotate_keys().await
    }

    async fn verify_peer(&self, peer_id: &str) -> Result<bool> {
        self.encryption_manager.verify_peer(peer_id).await
    }

    fn get_circuit_breaker(&self) -> Option<&CircuitBreaker> {
        self.circuit_breaker.as_ref()
    }
}

// Preserve existing TcpTransportStream implementation with enhancements
struct TcpTransportStream {
    stream: TcpStream,
    config: TransportConfig,
}

#[async_trait::async_trait]
impl TransportStream for TcpTransportStream {
    async fn set_timeout(&mut self, duration: Duration) -> Result<()> {
        self.stream.set_read_timeout(Some(duration))?;
        self.stream.set_write_timeout(Some(duration))?;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.stream.shutdown().await?;
        Ok(())
    }

    fn peer_address(&self) -> Option<String> {
        self.stream.peer_addr().ok().map(|addr| addr.to_string())
    }
}

// Implement AsyncRead and AsyncWrite using existing implementations
impl AsyncRead for TcpTransportStream {
    // ... [Keep existing implementation] ...
}

impl AsyncWrite for TcpTransportStream {
    // ... [Keep existing implementation] ...
}

// Preserve existing TcpTransportListener implementation with enhancements
struct TcpTransportListener {
    listener: TcpListener,
    config: TransportConfig,
    encryption_manager: Arc<EncryptionManager>,
}

#[async_trait::async_trait]
impl TransportListener for TcpTransportListener {
    async fn accept(&mut self) -> Result<Box<dyn TransportStream>> {
        let (stream, _) = self.listener.accept().await?;
        
        // Apply existing encryption if needed
        let encrypted_stream = if self.config.security_level != SecurityLevel::None {
            self.encryption_manager.encrypt_stream(stream).await?
        } else {
            stream
        };

        Ok(Box::new(TcpTransportStream {
            stream: encrypted_stream,
            config: self.config.clone(),
        }))
    }

    async fn shutdown(&mut self) -> Result<()> {
        // Use existing shutdown implementation
        self.listener.accept().await?;
        Ok(())
    }

    fn local_address(&self) -> Option<String> {
        self.listener.local_addr().ok().map(|addr| addr.to_string())
    }
}