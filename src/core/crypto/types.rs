// src/core/crypto/types.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub id: Uuid,
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub created_at: i64,
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKProof {
    pub proof: Vec<u8>,
    pub public_inputs: Vec<u8>,
    pub timestamp: i64,
}

