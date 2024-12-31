// src/network/transport/factory.rs
use super::{
    Transport, TransportType, 
    tcp::TcpTransport,
    tor::TorTransport,
    quic::QuicTransport,
    Result, TransportError
};
use std::net::SocketAddr;

pub struct TransportFactory;

impl TransportFactory {
    pub fn create(transport_type: TransportType, address: &str) -> Result<Box<dyn Transport>> {
        match transport_type {
            TransportType::Tcp => {
                Ok(Box::new(TcpTransport::new(address)))
            }
            TransportType::Tor => {
                Ok(Box::new(TorTransport::new(address)))
            }
            TransportType::Quic => {
                let addr = address.parse::<SocketAddr>()
                    .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
                Ok(Box::new(QuicTransport::new(addr)))
            }
        }
    }
}
