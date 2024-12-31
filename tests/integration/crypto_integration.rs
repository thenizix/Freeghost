// src/tests/integration/crypto_integration.rs

use crate::core::crypto::{
    secure_memory::SecureMemory,
    quantum::{QuantumResistantProcessor, SecurityLevel},
    types::{KeyPair, Signature, BiometricTemplate, TemplateType},
    audit::{AuditSystem, AuditEventType}
};

use tokio;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn test_complete_cryptographic_workflow() {
    // Initialize components
    let memory = SecureMemory::new(8192).expect("Failed to initialize secure memory");
    let processor = Arc::new(QuantumResistantProcessor::new(SecurityLevel::Standard)
        .expect("Failed to initialize quantum processor"));
    let audit = Arc::new(AuditSystem::new(30, SecurityLevel::Standard));
    
    // Test key generation with audit
    let keypair = {
        let event_id = audit.record_event(
            AuditEventType::KeyGeneration,
            None,
            None
        ).await.expect("Failed to record audit event");
        
        let pair = processor.generate_keypair()
            .expect("Failed to generate keypair");
            
        audit.record_event(
            AuditEventType::KeyGeneration,
            Some(pair.id),
            Some(serde_json::json!({
                "completed": true,
                "previous_event": event_id
            }))
        ).await.expect("Failed to record completion audit");
        
        pair
    };

    // Test secure memory operations with key material
    let test_data = b"test data for encryption";
    let mut memory_guard = memory.clone();
    memory_guard.write(test_data).expect("Failed to write to secure memory");
    
    let mut read_buffer = vec![0u8; test_data.len()];
    memory_guard.read(&mut read_buffer).expect("Failed to read from secure memory");
    assert_eq!(&read_buffer, test_data);

    // Test signing operation with audit
    let message = b"message to sign";
    let signature = {
        let event_id = audit.record_event(
            AuditEventType::SignatureCreation,
            Some(keypair.id),
            None
        ).await.expect("Failed to record signature audit");
        
        let sig = processor.sign(message, &keypair)
            .expect("Failed to create signature");
            
        audit.record_event(
            AuditEventType::SignatureCreation,
            Some(sig.id),
            Some(serde_json::json!({
                "completed": true,
                "previous_event": event_id,
                "keypair_id": keypair.id.to_string()
            }))
        ).await.expect("Failed to record signature completion");
        
        sig
    };

    // Test verification with audit
    let verification = {
        let event_id = audit.record_event(
            AuditEventType::SignatureVerification,
            Some(signature.id),
            None
        ).await.expect("Failed to record verification audit");
        
        let result = processor.verify(message, &signature, &keypair.public_key)
            .expect("Failed to verify signature");
            
        audit.record_event(
            AuditEventType::SignatureVerification,
            Some(signature.id),
            Some(serde_json::json!({
                "completed": true,
                "previous_event": event_id,
                "result": result.is_valid
            }))
        ).await.expect("Failed to record verification completion");
        
        result
    };
    
    assert!(verification.is_valid);

    // Test template generation and verification
    let template = BiometricTemplate::new(
        vec![0u8; 2048],
        TemplateType::Combined,
        SecurityLevel::Standard
    );
    
    template.verify().expect("Failed to verify template");

    // Verify audit trail
    let events = audit.get_events(
        chrono::Utc::now() - chrono::Duration::hours(1),
        chrono::Utc::now()
    ).await.expect("Failed to retrieve audit events");
    
    assert!(events.len() >= 6); // At least 6 events should have been recorded
    
    // Test audit summary
    let summary = audit.get_summary(
        chrono::Utc::now() - chrono::Duration::hours(1),
        chrono::Utc::now()
    ).await.expect("Failed to generate audit summary");
    
    assert!(summary.events_by_type.contains_key("KeyGeneration"));
    assert!(summary.events_by_type.contains_key("SignatureCreation"));
    assert!(summary.events_by_type.contains_key("SignatureVerification"));
}

#[tokio::test]
async fn test_error_conditions() {
    let processor = Arc::new(QuantumResistantProcessor::new(SecurityLevel::Standard)
        .expect("Failed to initialize quantum processor"));
    let audit = Arc::new(AuditSystem::new(30, SecurityLevel::Standard));

    // Test invalid key verification
    let invalid_keypair = KeyPair::new(vec![0u8; 100], SecurityLevel::Standard);
    assert!(invalid_keypair.verify().is_err());

    // Test invalid signature verification
    let valid_keypair = processor.generate_keypair().expect("Failed to generate keypair");
    let invalid_signature = Signature::new(
        valid_keypair.id,
        vec![0u8; 100]
    );
    
    let verify_result = processor.verify(
        b"test message",
        &invalid_signature,
        &valid_keypair.public_key
    );
    assert!(verify_result.is_err());

    // Test secure memory bounds
    let memory = SecureMemory::new(64).expect("Failed to initialize secure memory");
    let large_data = vec![0u8; 128];
    assert!(memory.write(&large_data).is_err());
}

#[tokio::test]
async fn test_security_level_changes() {
    let mut processor = QuantumResistantProcessor::new(SecurityLevel::Basic)
        .expect("Failed to initialize quantum processor");
    let audit = Arc::new(AuditSystem::new(30, SecurityLevel::Basic));

    // Generate keypair at basic security level
    let basic_keypair = processor.generate_keypair().expect("Failed to generate basic keypair");

    // Change security level
    processor.change_security_level(SecurityLevel::High);
    audit.record_event(
        AuditEventType::SecurityLevelChange,
        None,
        Some(serde_json::json!({
            "old_level": "Basic",
            "new_level": "High"
        }))
    ).await.expect("Failed to record security level change");

    // Generate keypair at high security level
    let high_keypair = processor.generate_keypair().expect("Failed to generate high keypair");

    // Verify different key sizes
    assert!(high_keypair.public_key.len() > basic_keypair.public_key.len());
}

#[tokio::test]
async fn test_concurrent_operations() {
    let processor = Arc::new(QuantumResistantProcessor::new(SecurityLevel::Standard)
        .expect("Failed to initialize quantum processor"));
    let audit = Arc::new(AuditSystem::new(30, SecurityLevel::Standard));

    // Spawn multiple concurrent operations
    let mut handles = vec![];
    
    for _ in 0..10 {
        let processor_clone = processor.clone();
        let audit_clone = audit.clone();
        
        handles.push(tokio::spawn(async move {
            let keypair = processor_clone.generate_keypair()
                .expect("Failed to generate keypair");
                
            audit_clone.record_event(
                AuditEventType::KeyGeneration,
                Some(keypair.id),
                None
            ).await.expect("Failed to record audit event");
            
            keypair
        }));
    }

    // Wait for all operations to complete
    let results = futures::future::join_all(handles).await;
    
    // Verify all operations succeeded
    for result in results {
        assert!(result.is_ok());
    }

    // Verify audit trail captured all operations
    let events = audit.get_events(
        chrono::Utc::now() - chrono::Duration::hours(1),
        chrono::Utc::now()
    ).await.expect("Failed to retrieve audit events");
    
    assert_eq!(events.len(), 10);
}