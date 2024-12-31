// src/network/transport/transport.rs
use async_trait::async_trait;
use std::time::Duration;
use super::{error::*, message::NetworkMessage};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportType {
    Tor,
    Tcp,
    Quic,
}

#[async_trait]
pub trait Transport: Send + Sync {
    fn transport_type(&self) -> TransportType;
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn send(&self, message: NetworkMessage) -> Result<()>;
    async fn receive(&self) -> Result<NetworkMessage>;
    async fn is_connected(&self) -> bool;
    async fn set_timeout(&mut self, timeout: Duration) -> Result<()>;
}
