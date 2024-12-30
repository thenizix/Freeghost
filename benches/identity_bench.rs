// benches/identity_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};
use secure_identity_node::core::identity::*;

fn bench_template_creation(c: &mut Criterion) {
    let processor = BiometricProcessor::new();
    let test_data = vec![1u8; 1024];
    
    c.bench_function("template_creation", |b| {
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                processor.process(&test_data).await.unwrap()
            })
        })
    });
}

fn bench_template_verification(c: &mut Criterion) {
    let service = VerificationService::new();
    let template = Template::default();
    
    c.bench_function("template_verification", |b| {
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                service.verify_template(&template).await.unwrap()
            })
        })
    });
}

criterion_group!(benches, bench_template_creation, bench_template_verification);
criterion_main!(benches);