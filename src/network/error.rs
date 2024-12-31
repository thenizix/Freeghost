// src/network/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("Message handling error: {0}")]
    MessageHandling(String),

    #[error("State error: {0}")]
    StateError(String),

    #[error("Invalid state version")]
    InvalidStateVersion,

    #[error("No majority state found")]
    NoMajorityState,

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Peer unreachable: {0}")]
    PeerUnreachable(String),

    #[error("Maximum retries exceeded")]
    MaxRetriesExceeded,

    #[error("Response timeout for peer: {0}")]
    ResponseTimeout(String),

    #[error("No handler found for message type: {0}")]
    NoHandlerFound(String),

    #[error("Temporary failure: {0}")]
    TemporaryFailure(String),

    #[error("State inconsistency detected")]
    StateInconsistency,

    #[error("All protocols failed: {0}")]
    AllProtocolsFailed(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, NetworkError>;