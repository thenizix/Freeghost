// benches/storage_benchmarks.rs
use criterion::{criterion_group, criterion_main, Criterion};
use secure_identity_node::{
    core::crypto::key_manager::KeyManager,
    storage::encrypted::EncryptedStore,
};
use tokio::runtime::Runtime;

fn bench_storage_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let temp_dir = tempfile::tempdir().unwrap();
    let store = rt.block_on(async {
        let key_manager = KeyManager::new_test_instance().await;
        EncryptedStore::new(temp_dir.path(), key_manager).await.unwrap()
    });
    
    let test_data = TestIdentity {
        id: "benchmark_test".to_string(),
        template: vec![1, 2, 3, 4],
        metadata: TestMetadata {
            created_at: 1000,
            updated_at: 1000,
            version: 1,
        },
    };
    
    // Benchmark write operations
    c.bench_function("store_write", |b| {
        b.iter(|| {
            rt.block_on(async {
                store
                    .store(test_data.id.as_bytes(), &test_data)
                    .await
                    .unwrap()
            })
        })
    });
    
    // Benchmark read operations
    c.bench_function("store_read", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _: TestIdentity = store
                    .retrieve(test_data.id.as_bytes())
                    .await
                    .unwrap()
                    .unwrap();
            })
        })
    });
    
    // Benchmark key rotation
    c.bench_function("key_rotation", |b| {
        b.iter(|| {
            rt.block_on(async {
                store.rotate_encryption_key().await.unwrap();
            })
        })
    });
}

criterion_group!(benches, bench_storage_operations);
criterion_main!(benches);