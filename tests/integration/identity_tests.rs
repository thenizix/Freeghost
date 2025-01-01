use freeghost::{
    core::{
        identity::types::{Identity, BiometricTemplate, DeviceInfo, BehaviorPattern, PatternType},
        services::identity::IdentityService,
        crypto::quantum::ZeroKnowledgeProof,
    },
    storage::encrypted::EncryptedStore,
    utils::config::{Config, StorageConfig, SecurityConfig},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_test_environment() -> (IdentityService, Arc<RwLock<EncryptedStore>>) {
    let temp_dir = tempdir().unwrap();
    
    let config = Config {
        node: Default::default(),
        network: Default::default(),
        storage: StorageConfig {
            path: temp_dir.path().to_str().unwrap().to_string(),
            encryption_key: "test_key".to_string(),
            max_size_gb: 1,
            backup_interval: 3600,
            compression_enabled: true,
        },
        plugins: Default::default(),
        security: SecurityConfig {
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            max_request_size: 1024 * 1024,
            rate_limit_requests: 100,
            rate_limit_window: 60,
        },
    };

    let storage = Arc::new(RwLock::new(
        EncryptedStore::new(&config.storage).await.unwrap()
    ));
    
    let service = IdentityService::new(&config, storage.clone()).await.unwrap();
    
    (service, storage)
}

#[tokio::test]
async fn test_identity_lifecycle() {
    let (service, _storage) = setup_test_environment().await;

    // Test identity creation
    let device_info = DeviceInfo {
        device_id: "test_device".to_string(),
        device_type: "mobile".to_string(),
        os_info: "Android 12".to_string(),
        first_seen: 0,
        last_seen: 0,
    };

    let biometric_data = vec![1, 2, 3, 4, 5]; // Mock biometric data
    let identity = service
        .create_identity(biometric_data.clone(), Some(device_info))
        .await
        .unwrap();

    assert!(identity.id != Uuid::nil());
    assert_eq!(identity.metadata.verification_count, 0);

    // Test identity verification
    let proof = ZeroKnowledgeProof {
        commitment: vec![0; 32],
        challenge: vec![0; 32],
        response: vec![0; 64],
    };

    let verified = service
        .verify_identity(identity.id, biometric_data.clone(), proof)
        .await
        .unwrap();

    assert!(verified);

    // Test behavior update
    let pattern = BehaviorPattern {
        pattern_type: PatternType::TimeOfDay,
        confidence: 0.8,
        occurrences: 1,
        last_seen: chrono::Utc::now().timestamp() as u64,
    };

    service.update_behavior(identity.id, pattern).await.unwrap();

    // Verify behavior update
    let updated_identity = service.get_identity(&identity.id).await.unwrap().unwrap();
    assert!(updated_identity.behavior_profile.trust_score > 0.0);
    assert_eq!(updated_identity.behavior_profile.patterns.len(), 1);

    // Test identity revocation
    service.revoke_identity(identity.id).await.unwrap();

    // Verify revocation
    let result = service
        .verify_identity(identity.id, biometric_data, proof)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_concurrent_identity_operations() {
    let (service, _storage) = setup_test_environment().await;
    let service = Arc::new(service);

    let mut handles = vec![];
    let num_identities = 10;

    // Create multiple identities concurrently
    for i in 0..num_identities {
        let service_clone = service.clone();
        let handle = tokio::spawn(async move {
            let biometric_data = vec![i as u8; 5];
            let device_info = DeviceInfo {
                device_id: format!("device_{}", i),
                device_type: "mobile".to_string(),
                os_info: "Android 12".to_string(),
                first_seen: 0,
                last_seen: 0,
            };

            service_clone
                .create_identity(biometric_data, Some(device_info))
                .await
                .unwrap()
        });
        handles.push(handle);
    }

    let identities: Vec<Identity> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(identities.len(), num_identities);

    // Verify all identities concurrently
    let mut verify_handles = vec![];
    for identity in identities {
        let service_clone = service.clone();
        let handle = tokio::spawn(async move {
            let proof = ZeroKnowledgeProof {
                commitment: vec![0; 32],
                challenge: vec![0; 32],
                response: vec![0; 64],
            };

            service_clone
                .verify_identity(
                    identity.id,
                    vec![0, 1, 2, 3, 4],
                    proof,
                )
                .await
                .unwrap()
        });
        verify_handles.push(handle);
    }

    let results: Vec<bool> = futures::future::join_all(verify_handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(results.len(), num_identities);
}

#[tokio::test]
async fn test_invalid_operations() {
    let (service, _storage) = setup_test_environment().await;

    // Test verification with non-existent identity
    let result = service
        .verify_identity(
            Uuid::new_v4(),
            vec![1, 2, 3],
            ZeroKnowledgeProof {
                commitment: vec![0; 32],
                challenge: vec![0; 32],
                response: vec![0; 64],
            },
        )
        .await;
    assert!(result.is_err());

    // Test behavior update with non-existent identity
    let result = service
        .update_behavior(
            Uuid::new_v4(),
            BehaviorPattern {
                pattern_type: PatternType::TimeOfDay,
                confidence: 0.8,
                occurrences: 1,
                last_seen: chrono::Utc::now().timestamp() as u64,
            },
        )
        .await;
    assert!(result.is_err());

    // Test revocation of non-existent identity
    let result = service.revoke_identity(Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_storage_persistence() {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        path: temp_dir.path().to_str().unwrap().to_string(),
        encryption_key: "test_key".to_string(),
        max_size_gb: 1,
        backup_interval: 3600,
        compression_enabled: true,
    };

    // Create and store an identity
    let identity = {
        let storage = Arc::new(RwLock::new(EncryptedStore::new(&config).await.unwrap()));
        let service = IdentityService::new(
            &Config {
                storage: config.clone(),
                ..Default::default()
            },
            storage,
        )
        .await
        .unwrap();

        let identity = service
            .create_identity(vec![1, 2, 3], None)
            .await
            .unwrap();
        
        identity
    };

    // Create new service instance and verify persistence
    let storage = Arc::new(RwLock::new(EncryptedStore::new(&config).await.unwrap()));
    let service = IdentityService::new(
        &Config {
            storage: config,
            ..Default::default()
        },
        storage,
    )
    .await
    .unwrap();

    let retrieved = service.get_identity(&identity.id).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, identity.id);
}
