// tests/integration/storage/encrypted_store_tests.rs
use secure_identity_node::{
    core::crypto::key_manager::KeyManager,
    storage::encrypted::{EncryptedStore, StorageError},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct TestIdentity {
    id: String,
    template: Vec<u8>,
    metadata: TestMetadata,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct TestMetadata {
    created_at: i64,
    updated_at: i64,
    version: u32,
}

async fn setup_test_store() -> (EncryptedStore, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let key_manager = KeyManager::new_test_instance().await;
    let store = EncryptedStore::new(temp_dir.path(), key_manager)
        .await
        .unwrap();
    (store, temp_dir)
}

#[tokio::test]
async fn test_concurrent_access() {
    let (store, _temp_dir) = setup_test_store().await;
    let store = std::sync::Arc::new(store);
    
    let mut handles = Vec::new();
    
    // Spawn multiple tasks doing simultaneous reads and writes
    for i in 0..10 {
        let store = store.clone();
        let handle = tokio::spawn(async move {
            let test_data = TestIdentity {
                id: format!("test_{}", i),
                template: vec![1, 2, 3, 4],
                metadata: TestMetadata {
                    created_at: 1000,
                    updated_at: 1000,
                    version: 1,
                },
            };
            
            // Write data
            store.store(test_data.id.as_bytes(), &test_data).await.unwrap();
            
            // Read data back
            let retrieved: TestIdentity = store
                .retrieve(test_data.id.as_bytes())
                .await
                .unwrap()
                .unwrap();
                
            assert_eq!(test_data, retrieved);
        });
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_key_rotation_durability() {
    let (store, _temp_dir) = setup_test_store().await;
    
    // Store multiple items
    let items: Vec<TestIdentity> = (0..5)
        .map(|i| TestIdentity {
            id: format!("test_{}", i),
            template: vec![1, 2, 3, 4],
            metadata: TestMetadata {
                created_at: 1000,
                updated_at: 1000,
                version: 1,
            },
        })
        .collect();
        
    for item in &items {
        store.store(item.id.as_bytes(), item).await.unwrap();
    }
    
    // Perform key rotation
    store.rotate_encryption_key().await.unwrap();
    
    // Verify all items are still accessible
    for item in &items {
        let retrieved: TestIdentity = store
            .retrieve(item.id.as_bytes())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(item, &retrieved);
    }
}

#[tokio::test]
async fn test_error_conditions() {
    let (store, _temp_dir) = setup_test_store().await;
    
    // Test retrieving non-existent key
    let result: Option<TestIdentity> = store.retrieve(b"nonexistent").await.unwrap();
    assert!(result.is_none());
    
    // Test storing invalid data
    let result = store.store(b"test", &vec![0xff, 0xff]).await;
    assert!(matches!(result, Err(StorageError::InvalidFormat(_))));
}