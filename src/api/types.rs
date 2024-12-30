// src/api/types.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct IdentityRequest {
    pub biometric_data: Vec<u8>,
    pub behavior_data: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct IdentityResponse {
    pub template_id: String,
    pub proof: Vec<u8>,
    pub timestamp: i64,
}

