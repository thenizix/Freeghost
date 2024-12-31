// src/network/transport/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Send error: {0}")]
    SendError(String),
    
    #[error("Receive error: {0}")]
    ReceiveError(String),
    
    #[error("Transport not available: {0}")]
    Unavailable(String),
    
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, TransportError>;
