// src/core/crypto/key_manager.rs
use super::types::KeyPair;
use crate::storage::encrypted::EncryptedStore;

pub struct KeyManager {
    store: EncryptedStore,
    quantum_processor: QuantumResistantProcessor,
}

impl KeyManager {
    pub fn new(store: EncryptedStore) -> Self {
        Self {
            store,
            quantum_processor: QuantumResistantProcessor::new(),
        }
    }

    pub async fn rotate_keys(&mut self) -> Result<KeyPair> {
        let new_keypair = self.quantum_processor.generate_keypair()?;
        self.store.store_keypair(&new_keypair).await?;
        Ok(new_keypair)
    }
}
