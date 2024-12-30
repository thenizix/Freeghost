// src/core/identity/biometric.rs
use async_trait::async_trait;
use crate::utils::error::{Result, NodeError};
use super::types::BiometricData;

#[async_trait]
pub trait BiometricProcessor: Send + Sync {
    async fn process(&self, data: &[u8]) -> Result<BiometricData>;
    async fn verify(&self, template: &BiometricData, input: &[u8]) -> Result<bool>;
}