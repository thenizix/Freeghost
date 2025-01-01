use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
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
use tempfile::tempdir;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
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

fn create_test_device_info() -> DeviceInfo {
    DeviceInfo {
        device_id: "test_device".to_string(),
        device_type: "mobile".to_string(),
        os_info: "Android 12".to_string(),
        first_seen: 0,
        last_seen: 0,
    }
}

fn bench_identity_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("identity_creation");

    // Test different biometric data sizes
    for size in [64, 128, 256].iter() {
        let (service, _) = rt.block_on(setup_test_environment());
        let biometric_data = vec![0u8; *size];
        let device_info = create_test_device_info();

        group.bench_with_input(
            BenchmarkId::new("create_identity", size),
            &biometric_data,
            |b, data| {
                b.iter(|| {
                    rt.block_on(async {
                        service
                            .create_identity(black_box(data.clone()), Some(device_info.clone()))
                            .await
                            .unwrap()
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_identity_verification(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("identity_verification");

    let (service, _) = rt.block_on(setup_test_environment());
    let biometric_data = vec![0u8; 128];
    let device_info = create_test_device_info();

    // Create test identity
    let identity = rt.block_on(async {
        service
            .create_identity(biometric_data.clone(), Some(device_info))
            .await
            .unwrap()
    });

    let proof = ZeroKnowledgeProof {
        commitment: vec![0; 32],
        challenge: vec![0; 32],
        response: vec![0; 64],
    };

    group.bench_function("verify_identity", |b| {
        b.iter(|| {
            rt.block_on(async {
                service
                    .verify_identity(
                        black_box(identity.id),
                        black_box(biometric_data.clone()),
                        black_box(proof.clone()),
                    )
                    .await
                    .unwrap()
            })
        });
    });

    group.finish();
}

fn bench_behavior_analysis(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("behavior_analysis");

    let (service, _) = rt.block_on(setup_test_environment());
    let identity = rt.block_on(async {
        service
            .create_identity(vec![0u8; 128], None)
            .await
            .unwrap()
    });

    // Test different numbers of behavior patterns
    for num_patterns in [1, 5, 10].iter() {
        let patterns: Vec<_> = (0..*num_patterns)
            .map(|i| BehaviorPattern {
                pattern_type: match i % 5 {
                    0 => PatternType::TimeOfDay,
                    1 => PatternType::Location,
                    2 => PatternType::DeviceUsage,
                    3 => PatternType::NetworkPattern,
                    _ => PatternType::InteractionStyle,
                },
                confidence: 0.8,
                occurrences: 1,
                last_seen: chrono::Utc::now().timestamp() as u64,
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("update_behavior", num_patterns),
            &patterns,
            |b, patterns| {
                b.iter(|| {
                    for pattern in patterns {
                        rt.block_on(async {
                            service
                                .update_behavior(black_box(identity.id), black_box(pattern.clone()))
                                .await
                                .unwrap()
                        });
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_concurrent_verifications(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrent_verifications");
    group.sample_size(10);

    let (service, _) = rt.block_on(setup_test_environment());
    let service = Arc::new(service);

    // Create test identities
    let identities: Vec<_> = rt
        .block_on(async {
            let mut ids = Vec::new();
            for _ in 0..10 {
                let id = service
                    .create_identity(vec![0u8; 128], None)
                    .await
                    .unwrap();
                ids.push(id);
            }
            ids
        });

    let proof = ZeroKnowledgeProof {
        commitment: vec![0; 32],
        challenge: vec![0; 32],
        response: vec![0; 64],
    };

    // Test different numbers of concurrent verifications
    for num_concurrent in [5, 10].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_verify", num_concurrent),
            num_concurrent,
            |b, &n| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = Vec::new();
                        let service = service.clone();

                        for i in 0..n {
                            let service = service.clone();
                            let id = identities[i as usize].id;
                            let proof = proof.clone();
                            handles.push(tokio::spawn(async move {
                                service
                                    .verify_identity(id, vec![0u8; 128], proof)
                                    .await
                                    .unwrap()
                            }));
                        }

                        futures::future::join_all(handles).await
                    })
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_identity_creation,
    bench_identity_verification,
    bench_behavior_analysis,
    bench_concurrent_verifications
);
criterion_main!(benches);
