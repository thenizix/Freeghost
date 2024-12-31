// src/core/crypto/key_manager.rs

use super::secure_memory::SecureMemory;
use super::quantum::{QuantumResistantProcessor, SecurityLevel};
use super::types::{KeyPair, CryptoMetadata};
use super::audit::{AuditSystem, AuditEventType};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum KeyManagerError {
    #[error("Key not found: {0}")]
    KeyNotFound(Uuid),
    #[error("Key rotation failed: {0}")]
    RotationFailed(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Memory error: {0}")]
    MemoryError(String),
    #[error("Quantum operation failed: {0}")]
    QuantumError(String),
    #[error("Audit error: {0}")]
    AuditError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPolicy {
    pub rotation_period: Duration,
    pub security_level: SecurityLevel,
    pub require_backup: bool,
    pub allowed_uses: Vec<KeyUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KeyUsage {
    Signing,
    Authentication,
    IdentityVerification,
    TemplateProtection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub created_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
    pub rotation_count: u32,
    pub usages: Vec<KeyUsage>,
    pub policy: KeyPolicy,
}

pub struct KeyManager {
    secure_memory: Arc<SecureMemory>,
    quantum_processor: Arc<QuantumResistantProcessor>,
    audit_system: Arc<AuditSystem>,
    active_keys: Arc<RwLock<HashMap<Uuid, (KeyPair, KeyMetadata)>>>,
    key_policies: Arc<RwLock<HashMap<KeyUsage, KeyPolicy>>>,
    metadata: Arc<RwLock<CryptoMetadata>>,
}

impl KeyManager {
    pub async fn new(
        secure_memory: Arc<SecureMemory>,
        quantum_processor: Arc<QuantumResistantProcessor>,
        audit_system: Arc<AuditSystem>,
        security_level: SecurityLevel,
    ) -> Result<Self, KeyManagerError> {
        let manager = Self {
            secure_memory,
            quantum_processor,
            audit_system,
            active_keys: Arc::new(RwLock::new(HashMap::new())),
            key_policies: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(CryptoMetadata::new(security_level))),
        };

        // Initialize default policies
        manager.initialize_default_policies().await?;

        // Start key rotation scheduler
        manager.start_rotation_scheduler();

        // Record initialization in audit
        manager.audit_system
            .record_event(
                AuditEventType::SystemStartup,
                None,
                Some(serde_json::json!({
                    "component": "KeyManager",
                    "security_level": format!("{:?}", security_level)
                }))
            )
            .await
            .map_err(|e| KeyManagerError::AuditError(e.to_string()))?;

        Ok(manager)
    }

    async fn initialize_default_policies(&self) -> Result<(), KeyManagerError> {
        let mut policies = self.key_policies.write().await;

        let default_policies = vec![
            (KeyUsage::Signing, KeyPolicy {
                rotation_period: Duration::days(30),
                security_level: SecurityLevel::High,
                require_backup: true,
                allowed_uses: vec![KeyUsage::Signing],
            }),
            (KeyUsage::Authentication, KeyPolicy {
                rotation_period: Duration::days(90),
                security_level: SecurityLevel::Standard,
                require_backup: true,
                allowed_uses: vec![KeyUsage::Authentication],
            }),
            (KeyUsage::IdentityVerification, KeyPolicy {
                rotation_period: Duration::days(180),
                security_level: SecurityLevel::High,
                require_backup: true,
                allowed_uses: vec![KeyUsage::IdentityVerification],
            }),
            (KeyUsage::TemplateProtection, KeyPolicy {
                rotation_period: Duration::days(365),
                security_level: SecurityLevel::High,
                require_backup: true,
                allowed_uses: vec![KeyUsage::TemplateProtection],
            }),
        ];

        for (usage, policy) in default_policies {
            policies.insert(usage, policy);
        }

        Ok(())
    }

    pub async fn generate_key(
        &self,
        usage: KeyUsage,
    ) -> Result<Uuid, KeyManagerError> {
        // Get policy for key usage
        let policies = self.key_policies.read().await;
        let policy = policies.get(&usage)
            .ok_or_else(|| KeyManagerError::KeyNotFound(Uuid::nil()))?;

        // Generate keypair
        self.quantum_processor.change_security_level(policy.security_level);
        let keypair = self.quantum_processor
            .generate_keypair()
            .map_err(|e| KeyManagerError::QuantumError(e.to_string()))?;

        // Create metadata
        let metadata = KeyMetadata {
            created_at: Utc::now(),
            last_used: Utc::now(),
            rotation_count: 0,
            usages: vec![usage.clone()],
            policy: policy.clone(),
        };

        // Store key
        self.active_keys.write().await
            .insert(keypair.id, (keypair.clone(), metadata));

        // Audit key generation
        self.audit_system
            .record_event(
                AuditEventType::KeyGeneration,
                Some(keypair.id),
                Some(serde_json::json!({
                    "usage": format!("{:?}", usage),
                    "security_level": format!("{:?}", policy.security_level)
                }))
            )
            .await
            .map_err(|e| KeyManagerError::AuditError(e.to_string()))?;

        Ok(keypair.id)
    }

    pub async fn get_key(
        &self,
        key_id: Uuid,
        usage: KeyUsage,
    ) -> Result<KeyPair, KeyManagerError> {
        let mut active_keys = self.active_keys.write().await;
        
        if let Some((keypair, metadata)) = active_keys.get_mut(&key_id) {
            // Verify usage is allowed
            if !metadata.policy.allowed_uses.contains(&usage) {
                return Err(KeyManagerError::KeyNotFound(key_id));
            }

            // Update last used timestamp
            metadata.last_used = Utc::now();

            // Audit key access
            self.audit_system
                .record_event(
                    AuditEventType::KeyGeneration,
                    Some(key_id),
                    Some(serde_json::json!({
                        "usage": format!("{:?}", usage),
                        "action": "access"
                    }))
                )
                .await
                .map_err(|e| KeyManagerError::AuditError(e.to_string()))?;

            Ok(keypair.clone())
        } else {
            Err(KeyManagerError::KeyNotFound(key_id))
        }
    }

    async fn rotate_key(
        &self,
        key_id: Uuid,
    ) -> Result<Uuid, KeyManagerError> {
        let mut active_keys = self.active_keys.write().await;
        
        if let Some((old_keypair, mut metadata)) = active_keys.remove(&key_id) {
            // Generate new keypair with same policy
            self.quantum_processor.change_security_level(metadata.policy.security_level);
            let new_keypair = self.quantum_processor
                .generate_keypair()
                .map_err(|e| KeyManagerError::QuantumError(e.to_string()))?;

            // Update metadata
            metadata.rotation_count += 1;
            metadata.created_at = Utc::now();
            metadata.last_used = Utc::now();

            // Store new key
            active_keys.insert(new_keypair.id, (new_keypair.clone(), metadata));

            // Audit key rotation
            self.audit_system
                .record_event(
                    AuditEventType::KeyRotation,
                    Some(new_keypair.id),
                    Some(serde_json::json!({
                        "old_key_id": key_id.to_string(),
                        "rotation_count": metadata.rotation_count
                    }))
                )
                .await
                .map_err(|e| KeyManagerError::AuditError(e.to_string()))?;

            Ok(new_keypair.id)
        } else {
            Err(KeyManagerError::KeyNotFound(key_id))
        }
    }

    fn start_rotation_scheduler(&self) {
        let key_manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::hours(1).to_std().unwrap());
            
            loop {
                interval.tick().await;
                if let Err(e) = key_manager.check_and_rotate_keys().await {
                    eprintln!("Key rotation error: {}", e);
                }
            }
        });
    }

    async fn check_and_rotate_keys(&self) -> Result<(), KeyManagerError> {
        let active_keys = self.active_keys.read().await;
        let now = Utc::now();

        for (key_id, (_, metadata)) in active_keys.iter() {
            if now - metadata.created_at > metadata.policy.rotation_period {
                // Release read lock before rotation
                drop(active_keys);
                self.rotate_key(*key_id).await?;
                break;
            }
        }

        Ok(())
    }

    pub async fn update_policy(
        &self,
        usage: KeyUsage,
        policy: KeyPolicy,
    ) -> Result<(), KeyManagerError> {
        let mut policies = self.key_policies.write().await;
        policies.insert(usage.clone(), policy.clone());

        // Audit policy update
        self.audit_system
            .record_event(
                AuditEventType::SecurityLevelChange,
                None,
                Some(serde_json::json!({
                    "usage": format!("{:?}", usage),
                    "new_policy": serde_json::to_string(&policy).unwrap()
                }))
            )
            .await
            .map_err(|e| KeyManagerError::AuditError(e.to_string()))?;

        Ok(())
    }
}

impl Clone for KeyManager {
    fn clone(&self) -> Self {
        Self {
            secure_memory: self.secure_memory.clone(),
            quantum_processor: self.quantum_processor.clone(),
            audit_system: self.audit_system.clone(),
            active_keys: self.active_keys.clone(),
            key_policies: self.key_policies.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    async fn create_test_manager() -> KeyManager {
        let secure_memory = Arc::new(SecureMemory::new(8192).unwrap());
        let quantum_processor = Arc::new(QuantumResistantProcessor::new(SecurityLevel::Standard).unwrap());
        let audit_system = Arc::new(AuditSystem::new(30, SecurityLevel::Standard));
        
        KeyManager::new(
            secure_memory,
            quantum_processor,
            audit_system,
            SecurityLevel::Standard,
        ).await.unwrap()
    }

    #[tokio::test]
    async fn test_key_generation_and_retrieval() {
        let manager = create_test_manager().await;
        
        let key_id = manager.generate_key(KeyUsage::Signing).await.unwrap();
        let keypair = manager.get_key(key_id, KeyUsage::Signing).await.unwrap();
        
        assert_eq!(keypair.id, key_id);
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let manager = create_test_manager().await;
        
        let key_id = manager.generate_key(KeyUsage::Signing).await.unwrap();
        let new_key_id = manager.rotate_key(key_id).await.unwrap();
        
        assert_ne!(key_id, new_key_id);
        assert!(manager.get_key(key_id, KeyUsage::Signing).await.is_err());
        assert!(manager.get_key(new_key_id, KeyUsage::Signing).await.is_ok());
    }

    #[tokio::test]
    async fn test_policy_update() {
        let manager = create_test_manager().await;
        
        let new_policy = KeyPolicy {
            rotation_period: Duration::days(15),
            security_level: SecurityLevel::High,
            require_backup: true,
            allowed_uses: vec![KeyUsage::Signing],
        };
        
        assert!(manager.update_policy(KeyUsage::Signing, new_policy).await.is_ok());
    }
}