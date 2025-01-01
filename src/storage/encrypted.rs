use std::path::Path;
use rocksdb::{DB, Options, WriteBatch};
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;
use tracing::{info, warn, error};

use crate::{
    utils::error::{Result, NodeError},
    core::{
        identity::types::Identity,
        crypto::key_manager::KeyManager,
    },
};

pub struct EncryptedStore {
    db: DB,
    key_manager: KeyManager,
}

impl EncryptedStore {
    pub async fn new(config: &crate::utils::config::StorageConfig) -> Result<Self> {
        let path = Path::new(&config.path);
        
        // Create directory if it doesn't exist
        if !path.exists() {
            std::fs::create_dir_all(path)
                .map_err(|e| NodeError::Storage(format!("Failed to create storage directory: {}", e)))?;
        }

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_max_total_wal_size(1024 * 1024 * 1024); // 1GB WAL
        opts.set_keep_log_file_num(10);
        opts.set_max_open_files(1000);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        
        // Open database
        let db = DB::open(&opts, path)
            .map_err(|e| NodeError::Storage(format!("Failed to open database: {}", e)))?;

        // Initialize key manager
        let key_manager = KeyManager::new(&config.encryption_key)?;

        Ok(Self {
            db,
            key_manager,
        })
    }

    pub async fn store_identity(&self, identity: &Identity) -> Result<()> {
        let key = format!("identity:{}", identity.id);
        self.store(&key, identity).await
    }

    pub async fn get_identity(&self, id: &Uuid) -> Result<Option<Identity>> {
        let key = format!("identity:{}", id);
        self.get(&key).await
    }

    pub async fn delete_identity(&self, id: &Uuid) -> Result<()> {
        let key = format!("identity:{}", id);
        self.delete(&key).await
    }

    pub async fn store<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        // Serialize value
        let serialized = serde_json::to_vec(value)
            .map_err(|e| NodeError::Storage(format!("Serialization failed: {}", e)))?;

        // Encrypt serialized data
        let encrypted = self.key_manager.encrypt(&serialized)?;

        // Store encrypted data
        self.db
            .put(key.as_bytes(), encrypted)
            .map_err(|e| NodeError::Storage(format!("Database write failed: {}", e)))?;

        Ok(())
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        // Retrieve encrypted data
        let encrypted = match self.db.get(key.as_bytes())
            .map_err(|e| NodeError::Storage(format!("Database read failed: {}", e)))? {
            Some(data) => data,
            None => return Ok(None),
        };

        // Decrypt data
        let decrypted = self.key_manager.decrypt(&encrypted)?;

        // Deserialize decrypted data
        let value = serde_json::from_slice(&decrypted)
            .map_err(|e| NodeError::Storage(format!("Deserialization failed: {}", e)))?;

        Ok(Some(value))
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        self.db
            .delete(key.as_bytes())
            .map_err(|e| NodeError::Storage(format!("Database delete failed: {}", e)))?;

        Ok(())
    }

    pub async fn batch_operation<F>(&self, operation: F) -> Result<()>
    where
        F: FnOnce(&mut WriteBatch) -> Result<()>,
    {
        let mut batch = WriteBatch::default();
        operation(&mut batch)?;

        self.db
            .write(batch)
            .map_err(|e| NodeError::Storage(format!("Batch operation failed: {}", e)))?;

        Ok(())
    }

    pub async fn backup(&self, backup_path: &Path) -> Result<()> {
        // Ensure backup directory exists
        if !backup_path.exists() {
            std::fs::create_dir_all(backup_path)
                .map_err(|e| NodeError::Storage(format!("Failed to create backup directory: {}", e)))?;
        }

        // Create backup
        let backup_engine = rocksdb::backup::BackupEngine::open(
            &rocksdb::backup::BackupEngineOptions::default(),
            backup_path,
        ).map_err(|e| NodeError::Storage(format!("Failed to create backup engine: {}", e)))?;

        backup_engine
            .create_new_backup(&self.db)
            .map_err(|e| NodeError::Storage(format!("Backup creation failed: {}", e)))?;

        info!("Created backup at {:?}", backup_path);
        Ok(())
    }

    pub async fn restore(&self, backup_path: &Path) -> Result<()> {
        // Verify backup exists
        if !backup_path.exists() {
            return Err(NodeError::Storage("Backup path does not exist".into()));
        }

        // Open backup engine
        let backup_engine = rocksdb::backup::BackupEngine::open(
            &rocksdb::backup::BackupEngineOptions::default(),
            backup_path,
        ).map_err(|e| NodeError::Storage(format!("Failed to open backup engine: {}", e)))?;

        // Close current database
        drop(&self.db);

        // Restore from backup
        backup_engine
            .restore_latest_backup(&self.db.path(), &self.db.path(), &rocksdb::backup::RestoreOptions::default())
            .map_err(|e| NodeError::Storage(format!("Restore failed: {}", e)))?;

        info!("Restored from backup at {:?}", backup_path);
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        // RocksDB will be closed when dropped
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::core::identity::types::BiometricTemplate;

    #[tokio::test]
    async fn test_identity_storage() {
        let temp_dir = tempdir().unwrap();
        let config = crate::utils::config::StorageConfig {
            path: temp_dir.path().to_str().unwrap().to_string(),
            encryption_key: "test_key".to_string(),
            max_size_gb: 1,
            backup_interval: 3600,
            compression_enabled: true,
        };

        let store = EncryptedStore::new(&config).await.unwrap();
        
        // Create test identity
        let template = BiometricTemplate::new(
            vec![0.1, 0.2, 0.3],
            0.9,
            "test_hash".to_string(),
        );
        let identity = Identity::new(template);
        let id = identity.id;

        // Store identity
        store.store_identity(&identity).await.unwrap();

        // Retrieve identity
        let retrieved = store.get_identity(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.id, id);

        // Delete identity
        store.delete_identity(&id).await.unwrap();
        assert!(store.get_identity(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_backup_restore() {
        let temp_dir = tempdir().unwrap();
        let backup_dir = tempdir().unwrap();
        let config = crate::utils::config::StorageConfig {
            path: temp_dir.path().to_str().unwrap().to_string(),
            encryption_key: "test_key".to_string(),
            max_size_gb: 1,
            backup_interval: 3600,
            compression_enabled: true,
        };

        let store = EncryptedStore::new(&config).await.unwrap();
        
        // Create and store test data
        store.store("test_key", &"test_value").await.unwrap();

        // Create backup
        store.backup(backup_dir.path()).await.unwrap();

        // Modify data
        store.store("test_key", &"modified_value").await.unwrap();

        // Restore from backup
        store.restore(backup_dir.path()).await.unwrap();

        // Verify restored data
        let value: String = store.get("test_key").await.unwrap().unwrap();
        assert_eq!(value, "test_value");
    }
}
