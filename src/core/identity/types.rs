// src/core/identity/types.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricData {
    pub id: Uuid,
    pub features: Vec<f64>,
    pub quality: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorProfile {
    pub id: Uuid,
    pub patterns: Vec<f64>,
    pub confidence: f64,
    pub last_updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: Uuid,
    pub biometric: BiometricData,
    pub behavior: BehaviorProfile,
    pub created_at: i64,
    pub version: u32,
}

