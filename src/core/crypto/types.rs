// src/core/crypto/types.rs

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use super::quantum::SecurityLevel;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoTypeError {
    #[error("Invalid key format")]
    InvalidKeyFormat,
    #[error("Invalid signature format")]
    InvalidSignatureFormat,
    #[error("Template generation failed")]
    TemplateGenerationFailed,
    #[error("Invalid template format")]
    InvalidTemplateFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub id: Uuid,
    pub public_key: Vec<u8>,
    pub created_at: i64,
    pub algorithm: String,
    pub security_level: SecurityLevel,
}

impl KeyPair {
    pub fn new(public_key: Vec<u8>, security_level: SecurityLevel) -> Self {
        Self {
            id: Uuid::new_v4(),
            public_key,
            created_at: chrono::Utc::now().timestamp(),
            algorithm: String::from("CRYSTALS-Dilithium"),
            security_level,
        }
    }

    pub fn verify(&self) -> Result<(), CryptoTypeError> {
        if self.public_key.is_empty() {
            return Err(CryptoTypeError::InvalidKeyFormat);
        }

        let expected_size = match self.security_level {
            SecurityLevel::Basic => 1312,
            SecurityLevel::Standard => 1952,
            SecurityLevel::High => 2592,
        };

        if self.public_key.len() != expected_size {
            return Err(CryptoTypeError::InvalidKeyFormat);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub id: Uuid,
    pub keypair_id: Uuid,
    pub signature_data: Vec<u8>,
    pub created_at: i64,
}

impl Signature {
    pub fn new(keypair_id: Uuid, signature_data: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            keypair_id,
            signature_data,
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn verify(&self, security_level: SecurityLevel) -> Result<(), CryptoTypeError> {
        if self.signature_data.is_empty() {
            return Err(CryptoTypeError::InvalidSignatureFormat);
        }

        let expected_size = match security_level {
            SecurityLevel::Basic => 2420,
            SecurityLevel::Standard => 3293,
            SecurityLevel::High => 4595,
        };

        if self.signature_data.len() != expected_size {
            return Err(CryptoTypeError::InvalidSignatureFormat);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub is_valid: bool,
    pub verified_at: i64,
    pub signature_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricTemplate {
    pub id: Uuid,
    pub template_data: Vec<u8>,
    pub template_type: TemplateType,
    pub created_at: i64,
    pub security_level: SecurityLevel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TemplateType {
    Facial,
    Fingerprint,
    Behavioral,
    Combined,
}

impl BiometricTemplate {
    pub fn new(template_data: Vec<u8>, template_type: TemplateType, security_level: SecurityLevel) -> Self {
        Self {
            id: Uuid::new_v4(),
            template_data,
            template_type,
            created_at: chrono::Utc::now().timestamp(),
            security_level,
        }
    }

    pub fn verify(&self) -> Result<(), CryptoTypeError> {
        if self.template_data.is_empty() {
            return Err(CryptoTypeError::InvalidTemplateFormat);
        }

        let min_size = match self.template_type {
            TemplateType::Facial => 512,
            TemplateType::Fingerprint => 256,
            TemplateType::Behavioral => 1024,
            TemplateType::Combined => 2048,
        };

        if self.template_data.len() < min_size {
            return Err(CryptoTypeError::InvalidTemplateFormat);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoMetadata {
    pub created_at: i64,
    pub updated_at: i64,
    pub algorithm_version: String,
    pub security_level: SecurityLevel,
    pub key_rotations: u32,
}

impl CryptoMetadata {
    pub fn new(security_level: SecurityLevel) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            created_at: now,
            updated_at: now,
            algorithm_version: String::from("CRYSTALS-Dilithium-v3.1"),
            security_level,
            key_rotations: 0,
        }
    }

    pub fn record_key_rotation(&mut self) {
        self.key_rotations += 1;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_verification() {
        let public_key = vec![0u8; 1312];
        let keypair = KeyPair::new(public_key, SecurityLevel::Basic);
        assert!(keypair.verify().is_ok());

        let invalid_keypair = KeyPair::new(vec![0u8; 100], SecurityLevel::Basic);
        assert!(invalid_keypair.verify().is_err());
    }

    #[test]
    fn test_signature_verification() {
        let signature = Signature::new(
            Uuid::new_v4(),
            vec![0u8; 2420],
        );
        assert!(signature.verify(SecurityLevel::Basic).is_ok());

        let invalid_signature = Signature::new(
            Uuid::new_v4(),
            vec![0u8; 100],
        );
        assert!(invalid_signature.verify(SecurityLevel::Basic).is_err());
    }

    #[test]
    fn test_biometric_template() {
        let template = BiometricTemplate::new(
            vec![0u8; 2048],
            TemplateType::Combined,
            SecurityLevel::High,
        );
        assert!(template.verify().is_ok());

        let invalid_template = BiometricTemplate::new(
            vec![0u8; 100],
            TemplateType::Combined,
            SecurityLevel::High,
        );
        assert!(invalid_template.verify().is_err());
    }

    #[test]
    fn test_crypto_metadata() {
        let mut metadata = CryptoMetadata::new(SecurityLevel::Standard);
        assert_eq!(metadata.key_rotations, 0);
        
        metadata.record_key_rotation();
        assert_eq!(metadata.key_rotations, 1);
        assert!(metadata.updated_at >= metadata.created_at);
    }
}