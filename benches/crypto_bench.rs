// benches/crypto_bench.rs
fn bench_quantum_operations(c: &mut Criterion) {
    let processor = QuantumResistantProcessor::new();
    let test_data = b"test message";
    
    c.bench_function("quantum_signing", |b| {
        let keypair = processor.generate_keypair().unwrap();
        b.iter(|| processor.sign(test_data, &keypair))
    });
}

criterion_group!(crypto_benches, bench_quantum_operations);
criterion_main!(crypto_benches);