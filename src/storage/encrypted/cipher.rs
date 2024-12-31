// src/storage/encrypted/cipher.rs
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use rand::{rngs::OsRng, RngCore};
use sha3::{Digest, Sha3_256};

pub struct StorageCipher {
    cipher: Aes256Gcm,
}

impl StorageCipher {
    pub fn new(key: &[u8]) -> Result<Self> {
        let hash = Sha3_256::digest(key);
        let cipher_key = Key::<Aes256Gcm>::from_slice(hash.as_slice());
        let cipher = Aes256Gcm::new(cipher_key);
        
        Ok(Self { cipher })
    }
    
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = self.cipher
            .encrypt(nonce, data)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;
        
        let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        
        Ok(result)
    }
    
    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        if encrypted_data.len() < 12 {
            return Err(StorageError::DecryptionError(
                "Invalid encrypted data length".to_string()
            ));
        }
        
        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| StorageError::DecryptionError(e.to_string()))
    }
}