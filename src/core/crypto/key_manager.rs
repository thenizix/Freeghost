use std::sync::RwLock;
use ring::{aead, digest, pbkdf2};
use sha3::{Sha3_256, Digest};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};

use crate::utils::error::{Result, NodeError};

const PBKDF2_ITERATIONS: u32 = 100_000;
const KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;

pub struct KeyManager {
    master_key: RwLock<Vec<u8>>,
    encryption_key: RwLock<Aes256Gcm>,
}

impl KeyManager {
    pub fn new(encryption_key: &str) -> Result<Self> {
        if encryption_key.is_empty() {
            return Err(NodeError::Crypto("Encryption key cannot be empty".into()));
        }

        // Generate master key using PBKDF2
        let mut salt = [0u8; SALT_LEN];
        ring::rand::SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| NodeError::Crypto("Failed to generate salt".into()))?;

        let mut master_key = vec![0u8; KEY_LEN];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            std::num::NonZeroU32::new(PBKDF2_ITERATIONS).unwrap(),
            &salt,
            encryption_key.as_bytes(),
            &mut master_key,
        );

        // Initialize AES-GCM cipher
        let cipher = Aes256Gcm::new_from_slice(&master_key)
            .map_err(|e| NodeError::Crypto(format!("Failed to initialize cipher: {}", e)))?;

        Ok(Self {
            master_key: RwLock::new(master_key),
            encryption_key: RwLock::new(cipher),
        })
    }

    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; 12];
        ring::rand::SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| NodeError::Crypto("Failed to generate nonce".into()))?;

        let nonce = Nonce::from_slice(&nonce);
        
        let cipher = self.encryption_key.read().unwrap();
        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| NodeError::Crypto(format!("Encryption failed: {}", e)))?;

        // Combine nonce and ciphertext
        let mut result = Vec::with_capacity(nonce.len() + ciphertext.len());
        result.extend_from_slice(nonce);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        if encrypted_data.len() < 12 {
            return Err(NodeError::Crypto("Invalid encrypted data".into()));
        }

        let (nonce, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce);

        let cipher = self.encryption_key.read().unwrap();
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| NodeError::Crypto(format!("Decryption failed: {}", e)))
    }

    pub fn hash_features(&self, features: &[f32]) -> Result<String> {
        // Convert features to bytes
        let mut bytes = Vec::with_capacity(features.len() * 4);
        for feature in features {
            bytes.extend_from_slice(&feature.to_le_bytes());
        }

        // Add master key as salt
        let master_key = self.master_key.read().unwrap();
        bytes.extend_from_slice(&master_key);

        // Use SHA3-256 for hashing
        let mut hasher = Sha3_256::new();
        hasher.update(&bytes);
        let result = hasher.finalize();

        Ok(hex::encode(result))
    }

    pub fn rotate_keys(&self) -> Result<()> {
        let mut new_key = vec![0u8; KEY_LEN];
        ring::rand::SystemRandom::new()
            .fill(&mut new_key)
            .map_err(|_| NodeError::Crypto("Failed to generate new key".into()))?;

        let new_cipher = Aes256Gcm::new_from_slice(&new_key)
            .map_err(|e| NodeError::Crypto(format!("Failed to initialize new cipher: {}", e)))?;

        // Update keys atomically
        {
            let mut master_key = self.master_key.write().unwrap();
            let mut encryption_key = self.encryption_key.write().unwrap();
            *master_key = new_key;
            *encryption_key = new_cipher;
        }

        Ok(())
    }

    pub fn derive_key(&self, purpose: &str) -> Result<Vec<u8>> {
        let master_key = self.master_key.read().unwrap();
        
        let mut context = digest::Context::new(&digest::SHA256);
        context.update(&master_key);
        context.update(purpose.as_bytes());
        
        let derived = context.finish();
        Ok(derived.as_ref().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption() {
        let key_manager = KeyManager::new("test_key").unwrap();
        let data = b"test data";
        
        let encrypted = key_manager.encrypt(data).unwrap();
        let decrypted = key_manager.decrypt(&encrypted).unwrap();
        
        assert_eq!(data.to_vec(), decrypted);
    }

    #[test]
    fn test_feature_hashing() {
        let key_manager = KeyManager::new("test_key").unwrap();
        let features = vec![0.1, 0.2, 0.3];
        
        let hash1 = key_manager.hash_features(&features).unwrap();
        let hash2 = key_manager.hash_features(&features).unwrap();
        
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_key_rotation() {
        let key_manager = KeyManager::new("test_key").unwrap();
        let data = b"test data";
        
        let encrypted = key_manager.encrypt(data).unwrap();
        key_manager.rotate_keys().unwrap();
        
        // Previous encrypted data should not be decryptable after rotation
        assert!(key_manager.decrypt(&encrypted).is_err());
    }
}
