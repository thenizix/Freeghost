use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use freeghost::{
    storage::encrypted::EncryptedStore,
    utils::config::StorageConfig,
    core::identity::types::{Identity, BiometricTemplate},
};
use tempfile::tempdir;
use uuid::Uuid;
use std::path::PathBuf;
use tokio::runtime::Runtime;

fn setup_test_store() -> (EncryptedStore, PathBuf) {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        path: temp_dir.path().to_str().unwrap().to_string(),
        encryption_key: "test_key".to_string(),
        max_size_gb: 1,
        backup_interval: 3600,
        compression_enabled: true,
    };

    let rt = Runtime::new().unwrap();
    let store = rt.block_on(async {
        EncryptedStore::new(&config).await.unwrap()
    });

    (store, temp_dir.path().to_owned())
}

fn create_test_identity() -> Identity {
    let template = BiometricTemplate::new(
        vec![0.1; 128],
        0.9,
        "test_hash".to_string(),
    );
    Identity::new(template)
}

fn bench_storage_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (store, _path) = setup_test_store();
    let identity = create_test_identity();

    // Single operation benchmarks
    c.bench_function("store_identity", |b| {
        b.iter(|| {
            rt.block_on(async {
                store.store_identity(black_box(&identity)).await.unwrap();
            });
        })
    });

    // Store the identity first
    rt.block_on(async {
        store.store_identity(&identity).await.unwrap();
    });

    c.bench_function("get_identity", |b| {
        b.iter(|| {
            rt.block_on(async {
                store.get_identity(black_box(&identity.id)).await.unwrap();
            });
        })
    });

    c.bench_function("delete_identity", |b| {
        b.iter(|| {
            rt.block_on(async {
                store.delete_identity(black_box(&identity.id)).await.unwrap();
            });
        })
    });
}

fn bench_batch_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("batch_operations");
    
    // Test different batch sizes
    for size in [10, 100, 1000].iter() {
        let (store, _path) = setup_test_store();
        let identities: Vec<_> = (0..*size).map(|_| create_test_identity()).collect();

        group.bench_with_input(BenchmarkId::new("store_batch", size), &identities, |b, ids| {
            b.iter(|| {
                rt.block_on(async {
                    store.batch_operation(|batch| {
                        for id in ids {
                            batch.put(
                                format!("identity:{}", id.id).as_bytes(),
                                serde_json::to_vec(id).unwrap(),
                            );
                        }
                        Ok(())
                    })
                    .await
                    .unwrap();
                });
            });
        });
    }

    group.finish();
}

fn bench_concurrent_access(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrent_access");
    group.sample_size(10);

    // Test different numbers of concurrent operations
    for num_concurrent in [5, 10, 20].iter() {
        let (store, _path) = setup_test_store();
        let store = std::sync::Arc::new(store);
        let identities: Vec<_> = (0..*num_concurrent).map(|_| create_test_identity()).collect();

        // Store identities first
        rt.block_on(async {
            for identity in &identities {
                store.store_identity(identity).await.unwrap();
            }
        });

        group.bench_with_input(
            BenchmarkId::new("concurrent_read", num_concurrent),
            &identities,
            |b, ids| {
                b.iter(|| {
                    rt.block_on(async {
                        let store = store.clone();
                        let mut handles = Vec::new();

                        for id in ids {
                            let store = store.clone();
                            let id = id.id;
                            handles.push(tokio::spawn(async move {
                                store.get_identity(&id).await.unwrap();
                            }));
                        }

                        futures::future::join_all(handles).await;
                    });
                });
            },
        );
    }

    group.finish();
}

fn bench_backup_restore(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("backup_restore");
    group.sample_size(10);

    // Test with different amounts of data
    for num_identities in [100, 1000].iter() {
        let (store, path) = setup_test_store();
        let backup_path = path.join("backup");
        let identities: Vec<_> = (0..*num_identities).map(|_| create_test_identity()).collect();

        // Store test data
        rt.block_on(async {
            for identity in &identities {
                store.store_identity(identity).await.unwrap();
            }
        });

        group.bench_with_input(
            BenchmarkId::new("backup", num_identities),
            &backup_path,
            |b, backup_path| {
                b.iter(|| {
                    rt.block_on(async {
                        store.backup(black_box(backup_path)).await.unwrap();
                    });
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("restore", num_identities),
            &backup_path,
            |b, backup_path| {
                b.iter(|| {
                    rt.block_on(async {
                        store.restore(black_box(backup_path)).await.unwrap();
                    });
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_storage_operations,
    bench_batch_operations,
    bench_concurrent_access,
    bench_backup_restore
);
criterion_main!(benches);
