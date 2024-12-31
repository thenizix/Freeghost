
// src/storage/encrypted/store.rs
use super::{cipher::StorageCipher, errors::*};
use crate::core::crypto::key_manager::KeyManager;
use rocksdb::{DB, Options};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::RwLock;
use std::path::Path;

pub struct EncryptedStore {
    db: DB,
    cipher: RwLock<StorageCipher>,
    key_manager: KeyManager,
}

impl EncryptedStore {
    pub async fn new<P: AsRef<Path>>(
        path: P,
        key_manager: KeyManager
    ) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        
        let db = DB::open(&opts, path)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))?;
            
        let current_key = key_manager.current_key().await?;
        let cipher = StorageCipher::new(current_key.as_slice())?;
        
        Ok(Self {
            db,
            cipher: RwLock::new(cipher),
            key_manager,
        })
    }
    
    pub async fn store<T: Serialize>(&self, key: &[u8], value: &T) -> Result<()> {
        let serialized = serde_json::to_vec(value)
            .map_err(|e| StorageError::InvalidFormat(e.to_string()))?;
            
        let cipher = self.cipher.read().await;
        let encrypted = cipher.encrypt(&serialized)?;
        
        self.db
            .put(key, encrypted)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))?;
            
        Ok(())
    }
    
    pub async fn retrieve<T: DeserializeOwned>(&self, key: &[u8]) -> Result<Option<T>> {
        let encrypted = match self.db.get(key)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))? {
            Some(data) => data,
            None => return Ok(None),
        };
        
        let cipher = self.cipher.read().await;
        let decrypted = cipher.decrypt(&encrypted)?;
        
        let value = serde_json::from_slice(&decrypted)
            .map_err(|e| StorageError::InvalidFormat(e.to_string()))?;
            
        Ok(Some(value))
    }
    
    pub async fn delete(&self, key: &[u8]) -> Result<()> {
        self.db
            .delete(key)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))?;
        Ok(())
    }
    
    pub async fn rotate_encryption_key(&self) -> Result<()> {
        let new_key = self.key_manager.rotate_keys().await?;
        let new_cipher = StorageCipher::new(new_key.as_slice())?;
        
        // Re-encrypt all existing data with new key
        let mut batch = rocksdb::WriteBatch::default();
        let iter = self.db.iterator(rocksdb::IteratorMode::Start);
        
        let old_cipher = self.cipher.read().await;
        
        for item in iter {
            let (key, old_encrypted) = item
                .map_err(|e| StorageError::DatabaseError(e.to_string()))?;
                
            let decrypted = old_cipher.decrypt(&old_encrypted)?;
            let new_encrypted = new_cipher.encrypt(&decrypted)?;
            
            batch.put(&key, &new_encrypted);
        }
        
        self.db
            .write(batch)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))?;
            
        *self.cipher.write().await = new_cipher;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_store_and_retrieve() {
        let temp_dir = tempdir().unwrap();
        let key_manager = KeyManager::new_test_instance().await;
        let store = EncryptedStore::new(temp_dir.path(), key_manager)
            .await
            .unwrap();
            
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct TestData {
            field1: String,
            field2: i32,
        }
        
        let test_data = TestData {
            field1: "test".to_string(),
            field2: 42,
        };
        
        store.store(b"test_key", &test_data).await.unwrap();
        let retrieved: TestData = store.retrieve(b"test_key").await.unwrap().unwrap();
        
        assert_eq!(test_data, retrieved);
    }
    
    #[tokio::test]
    async fn test_key_rotation() {
        let temp_dir = tempdir().unwrap();
        let key_manager = KeyManager::new_test_instance().await;
        let store = EncryptedStore::new(temp_dir.path(), key_manager)
            .await
            .unwrap();
            
        // Store data with original key
        store.store(b"test_key", &"test_value").await.unwrap();
        
        // Rotate key and verify data is still accessible
        store.rotate_encryption_key().await.unwrap();
        let retrieved: String = store.retrieve(b"test_key").await.unwrap().unwrap();
        
        assert_eq!("test_value", retrieved);
    }
}