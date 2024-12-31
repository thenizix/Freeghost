// src/network/protocol.rs
pub struct MessageProtocol {
    quantum_crypto: Arc<QuantumResistantProcessor>,
    version: u32,
}

impl MessageProtocol {
    pub fn new(quantum_crypto: Arc<QuantumResistantProcessor>) -> Self {
        Self {
            quantum_crypto,
            version: 1,
        }
    }

    pub async fn encode_message(&self, payload: &[u8]) -> Result<SecureMessage> {
        let id = Uuid::new_v4();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Generate quantum-resistant signature
        let signature = self.quantum_crypto.sign_data(payload)?;
        
        // Generate secure nonce
        let nonce = self.quantum_crypto.generate_nonce()?;

        // Encrypt payload with quantum-resistant encryption
        let encrypted_payload = self.quantum_crypto.encrypt(payload, &nonce)?;

        Ok(SecureMessage {
            id,
            timestamp,
            signature,
            payload: encrypted_payload,
            nonce,
            protocol_version: self.version,
        })
    }

    pub async fn decode_message(&self, message: SecureMessage) -> Result<Vec<u8>> {
        // Verify signature
        self.quantum_crypto.verify_signature(&message.signature, &message.payload)?;

        // Decrypt payload
        let decrypted = self.quantum_crypto.decrypt(&message.payload, &message.nonce)?;

        Ok(decrypted)
    }
}