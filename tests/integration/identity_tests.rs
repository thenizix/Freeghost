// tests/integration/identity_tests.rs
use crate::common::TestContext;

#[tokio::test]
async fn test_identity_creation_and_verification() {
    let ctx = TestContext::new().await;
    let test_data = vec![1, 2, 3, 4];
    
    // Create identity
    let template = ctx.identity_service
        .create_identity(test_data)
        .await
        .expect("Failed to create identity");
        
    // Verify template
    let result = ctx.verification_service
        .verify_template(&template)
        .await
        .expect("Failed to verify template");
        
    assert!(result);
}

#[tokio::test]
async fn test_identity_storage_and_retrieval() {
    let ctx = TestContext::new().await;
    let test_data = vec![1, 2, 3, 4];
    
    // Create and store template
    let template = ctx.identity_service
        .create_identity(test_data)
        .await
        .expect("Failed to create identity");
        
    ctx.storage
        .store_template(&template)
        .await
        .expect("Failed to store template");
        
    // Retrieve and verify
    let retrieved = ctx.storage
        .get_template(template.id)
        .await
        .expect("Failed to retrieve template");
        
    assert_eq!(template.id, retrieved.id);
}
