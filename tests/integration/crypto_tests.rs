// tests/integration/crypto_tests.rs
#[tokio::test]
async fn test_quantum_resistant_operations() {
    let processor = QuantumResistantProcessor::new();
    let test_data = b"test message";
    
    let keypair = processor
        .generate_keypair()
        .expect("Failed to generate keypair");
        
    let signature = processor
        .sign(test_data, &keypair)
        .expect("Failed to sign data");
        
    let valid = processor
        .verify(test_data, &signature, &keypair.public_key)
        .expect("Failed to verify signature");
        
    assert!(valid);
}