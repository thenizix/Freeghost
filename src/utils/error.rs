use thiserror::Error;

#[derive(Error, Debug)]
pub enum NodeError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Identity error: {0}")]
    Identity(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Initialization error: {0}")]
    Init(String),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub type Result<T> = std::result::Result<T, NodeError>;
