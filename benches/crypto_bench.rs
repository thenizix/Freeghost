use criterion::{black_box, criterion_group, criterion_main, Criterion};
use freeghost::core::crypto::{
    key_manager::KeyManager,
    quantum::QuantumResistantProcessor,
};

fn bench_key_manager(c: &mut Criterion) {
    let key_manager = KeyManager::new("test_encryption_key").unwrap();
    let test_data = vec![0u8; 1024]; // 1KB of test data

    c.bench_function("encrypt 1KB", |b| {
        b.iter(|| {
            key_manager.encrypt(black_box(&test_data)).unwrap();
        })
    });

    let encrypted = key_manager.encrypt(&test_data).unwrap();
    c.bench_function("decrypt 1KB", |b| {
        b.iter(|| {
            key_manager.decrypt(black_box(&encrypted)).unwrap();
        })
    });

    let features = vec![0.1f32; 128];
    c.bench_function("hash features", |b| {
        b.iter(|| {
            key_manager.hash_features(black_box(&features)).unwrap();
        })
    });

    c.bench_function("key rotation", |b| {
        b.iter(|| {
            key_manager.rotate_keys().unwrap();
        })
    });
}

fn bench_quantum_resistant(c: &mut Criterion) {
    let processor = QuantumResistantProcessor::new().unwrap();
    let features = vec![0.1f32; 128];

    c.bench_function("generate keypair", |b| {
        b.iter(|| {
            processor.generate_keypair().unwrap();
        })
    });

    let (_, private_key) = processor.generate_keypair().unwrap();
    c.bench_function("create zkp", |b| {
        b.iter(|| {
            processor.create_zkp(black_box(&features), black_box(&private_key)).unwrap();
        })
    });

    let proof = processor.create_zkp(&features, &private_key).unwrap();
    c.bench_function("verify zkp", |b| {
        b.iter(|| {
            processor
                .verify_zkp(black_box(&proof), black_box(&features), black_box("test_hash"))
                .unwrap();
        })
    });

    c.bench_function("refresh entropy", |b| {
        b.iter(|| {
            processor.refresh_entropy().unwrap();
        })
    });
}

fn bench_large_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_data");
    group.sample_size(10); // Reduce sample size for large data tests

    let key_manager = KeyManager::new("test_encryption_key").unwrap();
    
    // Test with different data sizes
    let sizes = [64 * 1024, 256 * 1024, 1024 * 1024]; // 64KB, 256KB, 1MB
    
    for size in sizes.iter() {
        let test_data = vec![0u8; *size];
        
        group.bench_function(format!("encrypt {}KB", size / 1024), |b| {
            b.iter(|| {
                key_manager.encrypt(black_box(&test_data)).unwrap();
            })
        });

        let encrypted = key_manager.encrypt(&test_data).unwrap();
        group.bench_function(format!("decrypt {}KB", size / 1024), |b| {
            b.iter(|| {
                key_manager.decrypt(black_box(&encrypted)).unwrap();
            })
        });
    }

    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    let mut group = c.benchmark_group("concurrent_operations");
    group.sample_size(10);

    let rt = Runtime::new().unwrap();
    let key_manager = Arc::new(KeyManager::new("test_encryption_key").unwrap());
    let test_data = vec![0u8; 1024];

    group.bench_function("concurrent_encrypt_10", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();
                let km = key_manager.clone();
                
                for _ in 0..10 {
                    let data = test_data.clone();
                    let km = km.clone();
                    handles.push(tokio::spawn(async move {
                        km.encrypt(&data).unwrap()
                    }));
                }

                futures::future::join_all(handles).await
            })
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_key_manager,
    bench_quantum_resistant,
    bench_large_data,
    bench_concurrent_operations
);
criterion_main!(benches);
