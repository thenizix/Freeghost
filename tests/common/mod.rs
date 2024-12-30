// tests/common/mod.rs
use secure_identity_node::{
    core::{identity::*, crypto::*},
    network::*,
    storage::*,
};

pub struct TestContext {
    pub identity_service: IdentityService,
    pub verification_service: VerificationService,
    pub storage: EncryptedStore,
}

impl TestContext {
    pub async fn new() -> Self {
        let config = Config::new_test_config();
        let storage = EncryptedStore::new(config.storage, &[0u8; 32]).unwrap();
        
        Self {
            identity_service: IdentityService::new(),
            verification_service: VerificationService::new(),
            storage,
        }
    }
}
