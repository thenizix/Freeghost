// src/utils/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Identity error: {0}")]
    Identity(String),
    
    #[error("Crypto error: {0}")]
    Crypto(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Plugin error: {0}")]
    Plugin(String),
}

pub type Result<T> = std::result::Result<T, NodeError>;

