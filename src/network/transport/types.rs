// src/network/transport/types.rs

use std::fmt::Debug;
use std::time::Duration;
use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::core::crypto::types::SecurityLevel;
use crate::network::circuit_breaker::CircuitBreaker;
use crate::utils::error::Result;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Connection timeout")]
    Timeout,
    #[error("Circuit breaker open")]
    CircuitBreakerOpen,
    #[error("Transport encryption failed: {0}")]
    EncryptionError(String),
    #[error("Transport protocol error: {0}")]
    ProtocolError(String),
    #[error("Transport authentication failed")]
    AuthenticationFailed,
}

#[derive(Debug, Clone)]
pub struct TransportConfig {
    pub security_level: SecurityLevel,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub max_frame_size: usize,
    pub keep_alive_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct TransportMetrics {
    pub latency: Duration,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub active_connections: u32,
    pub error_count: u32,
}

// Enhanced to support the existing implementations
#[async_trait]
pub trait Transport: Send + Sync + Debug {
    // Core transport operations
    async fn connect(&self, address: &str) -> Result<Box<dyn TransportStream>>;
    async fn listen(&self, address: &str) -> Result<Box<dyn TransportListener>>;
    
    // Configuration and metrics
    fn get_config(&self) -> &TransportConfig;
    async fn get_metrics(&self) -> Result<TransportMetrics>;
    
    // Security operations from existing implementation
    async fn rotate_keys(&self) -> Result<()>;
    async fn verify_peer(&self, peer_id: &str) -> Result<bool>;
    
    // Circuit breaker integration
    fn get_circuit_breaker(&self) -> Option<&CircuitBreaker>;
}

#[async_trait]
pub trait TransportStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {
    async fn set_timeout(&mut self, duration: Duration) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
    fn peer_address(&self) -> Option<String>;
}

#[async_trait]
pub trait TransportListener: Send + Sync {
    async fn accept(&mut self) -> Result<Box<dyn TransportStream>>;
    async fn shutdown(&mut self) -> Result<()>;
    fn local_address(&self) -> Option<String>;
}

// Keep existing traits that we found in the codebase
#[async_trait]
pub trait EncryptedTransport: Transport {
    async fn negotiate_keys(&mut self) -> Result<()>;
    async fn rotate_session_key(&mut self) -> Result<()>;
}

#[async_trait]
pub trait AnonymousTransport: Transport {
    async fn build_circuit(&mut self) -> Result<()>;
    async fn change_circuit(&mut self) -> Result<()>;
    fn get_circuit_id(&self) -> Option<String>;
}

// Add support for existing protocol-specific configurations
#[derive(Debug, Clone)]
pub struct TcpConfig {
    pub nodelay: bool,
    pub keepalive: Option<Duration>,
    pub send_buffer_size: usize,
    pub recv_buffer_size: usize,
}

#[derive(Debug, Clone)]
pub struct QuicConfig {
    pub max_concurrent_streams: u32,
    pub max_stream_data: u64,
    pub idle_timeout: Duration,
    pub keep_alive_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct TorConfig {
    pub circuit_build_timeout: Duration,
    pub circuit_idle_timeout: Duration,
    pub enforce_distinct_subnets: bool,
    pub circuit_hop_count: u8,
}

// Implement conversion traits for existing code
impl From<TransportError> for crate::utils::error::Error {
    fn from(err: TransportError) -> Self {
        crate::utils::error::Error::Transport(err.to_string())
    }
}