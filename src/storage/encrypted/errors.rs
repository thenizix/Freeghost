/ src/storage/encrypted/errors.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    #[error("Decryption error: {0}")]
    DecryptionError(String),
    
    #[error("Key management error: {0}")]
    KeyError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Invalid data format: {0}")]
    InvalidFormat(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;
