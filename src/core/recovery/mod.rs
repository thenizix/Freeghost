// src/core/recovery/mod.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use sha3::{Sha3_512, Digest};
use ring::aead;
use std::time::{SystemTime, Duration, UNIX_EPOCH};
use tracing::{info, warn, error};
use thiserror::Error;

// Re-exports from other modules
use crate::core::crypto::types::SecurityLevel;
use crate::core::biometrics::types::BiometricFactor;
use crate::utils::error::Result;

#[derive(Debug, Clone)]
pub struct RecoveryToken {
    blinded_id: [u8; 32],
    encrypted_data: Vec<u8>,
    epoch: u64,
    proof: RecoveryProof,
}

#[derive(Debug, Clone)]
pub struct RecoveryProof {
    data: Vec<u8>,
    timestamp: SystemTime,
    verification_hash: [u8; 64],
}

pub struct SecureRecoveryManager {
    encryption_key: aead::SealingKey<aead::Aes256Gcm>,
    decryption_key: aead::OpeningKey<aead::Aes256Gcm>,
    active_tokens: Arc<RwLock<HashMap<[u8; 32], RecoveryToken>>>,
    epoch_manager: EpochManager,
}

#[derive(Debug, Clone)]
pub struct RecoveryContext {
    purpose: RecoveryPurpose,
    timestamp: SystemTime,
    device_info: DeviceInfo,
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    hardware_hash: [u8; 32],
    security_level: SecurityLevel,
    verification_factors: Vec<VerificationFactor>,
}

#[derive(Debug, Clone, Copy)]
pub enum RecoveryPurpose {
    LostDevice,
    CompromisedCredentials,
    SystemFailure,
}

#[derive(Debug, Clone, Copy)]
pub enum VerificationFactor {
    Biometric(BiometricFactor),
    Knowledge,
    Possession,
}

impl SecureRecoveryManager {
    pub fn new(encryption_key: &[u8; 32]) -> Result<Self, RecoveryError> {
        let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, encryption_key)
            .map_err(|_| RecoveryError::KeyGenerationError)?;
        
        let sealing_key = aead::SealingKey::new(unbound_key.clone());
        let opening_key = aead::OpeningKey::new(unbound_key);

        Ok(Self {
            encryption_key: sealing_key,
            decryption_key: opening_key,
            active_tokens: Arc::new(RwLock::new(HashMap::new())),
            epoch_manager: EpochManager::new(),
        })
    }

    // ... [Previous methods remain the same] ...

    async fn verify_recovery_token(
        &self,
        token: &RecoveryToken,
        context: &RecoveryContext,
    ) -> Result<(), RecoveryError> {
        // Verify epoch
        if token.epoch != self.epoch_manager.current_epoch() {
            return Err(RecoveryError::ExpiredToken);
        }

        // Verify proof
        self.verify_recovery_proof(&token.proof, &token.blinded_id, context)
            .await?;

        // Verify token existence
        let active_tokens = self.active_tokens.read().await;
        if !active_tokens.contains_key(&token.blinded_id) {
            return Err(RecoveryError::InvalidToken);
        }

        Ok(())
    }
}

impl RecoveryContext {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Add timestamp
        bytes.extend_from_slice(&self.timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_le_bytes());
            
        // Add device info
        bytes.extend_from_slice(&self.device_info.hardware_hash);
        bytes.extend_from_slice(&[self.device_info.security_level as u8]);
        
        // Add purpose
        bytes.extend_from_slice(&[self.purpose as u8]);
        
        // Add verification factors
        for factor in &self.device_info.verification_factors {
            bytes.push(*factor as u8);
        }
        
        bytes
    }
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("Failed to generate key")]
    KeyGenerationError,
    #[error("Encryption failed")]
    EncryptionError,
    #[error("Decryption failed")]
    DecryptionError,
    #[error("Invalid recovery token")]
    InvalidToken,
    #[error("Expired recovery token")]
    ExpiredToken,
    #[error("Invalid recovery proof")]
    InvalidProof,
    #[error("Expired recovery proof")]
    ExpiredProof,
    #[error("Timestamp error")]
    TimestampError,
}

struct EpochManager {
    current_epoch: Arc<RwLock<u64>>,
    last_rotation: Arc<RwLock<SystemTime>>,
}

impl EpochManager {
    fn new() -> Self {
        Self {
            current_epoch: Arc::new(RwLock::new(0)),
            last_rotation: Arc::new(RwLock::new(SystemTime::now())),
        }
    }

    fn current_epoch(&self) -> u64 {
        // Using blocking read for simplicity in epoch checking
        // In a production environment, use proper async handling
        futures::executor::block_on(self.current_epoch.read()).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_recovery_token_generation_and_verification() {
        let key = [0u8; 32];
        let manager = SecureRecoveryManager::new(&key).unwrap();

        let recovery_data = b"test recovery data";
        let context = RecoveryContext {
            purpose: RecoveryPurpose::LostDevice,
            timestamp: SystemTime::now(),
            device_info: DeviceInfo {
                hardware_hash: [0u8; 32],
                security_level: SecurityLevel::High,
                verification_factors: vec![VerificationFactor::Biometric(BiometricFactor::Fingerprint)],
            },
        };

        // Generate recovery token
        let token = manager.generate_recovery_token(recovery_data, &context)
            .await
            .unwrap();

        // Verify and recover data
        let recovered_data = manager.recover_access(&token, &context)
            .await
            .unwrap();

        assert_eq!(recovery_data, recovered_data.as_slice());
    }
}