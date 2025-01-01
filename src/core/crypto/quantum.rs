use std::sync::RwLock;
use ring::rand::SystemRandom;
use sha3::{Sha3_256, Digest};

use crate::{
    utils::error::{Result, NodeError},
    core::crypto::{
        kyber::{KyberKEM, PublicKey as KyberPublicKey, SecretKey as KyberSecretKey},
        dilithium::{Dilithium, PublicKey as DilithiumPublicKey, SecretKey as DilithiumSecretKey, Signature},
        serialization::{
            serialize_public_key, deserialize_public_key,
            serialize_secret_key, deserialize_secret_key,
            serialize_ciphertext, deserialize_ciphertext,
        },
    },
};

pub struct QuantumResistantProcessor {
    rng: SystemRandom,
    state: RwLock<ProcessorState>,
    kyber_cache: RwLock<KyberCache>,
}

#[derive(Default)]
struct ProcessorState {
    current_round: u64,
    entropy_pool: Vec<u8>,
}

#[derive(Default)]
struct KyberCache {
    public_key: Option<KyberPublicKey>,
    secret_key: Option<KyberSecretKey>,
}

pub struct ZeroKnowledgeProof {
    pub commitment: Vec<u8>,
    pub challenge: Vec<u8>,
    pub response: Vec<u8>,
}

#[derive(Debug)]
pub enum QuantumAlgorithm {
    Kyber,
    Dilithium,
    // Future: Add SPHINCS+, etc.
}

impl QuantumResistantProcessor {
    pub fn new() -> Result<Self> {
        Ok(Self {
            rng: SystemRandom::new(),
            state: RwLock::new(ProcessorState {
                current_round: 0,
                entropy_pool: Vec::with_capacity(1024),
            }),
            kyber_cache: RwLock::new(KyberCache::default()),
        })
    }

    pub fn generate_keypair(&self) -> Result<((Vec<u8>, Vec<u8>), (Vec<u8>, Vec<u8>))> {
        // Generate Kyber keypair
        let (pk, sk) = KyberKEM::keygen()?;
        
        // Cache the keypair for later use
        let mut cache = self.kyber_cache.write().unwrap();
        cache.public_key = Some(pk.clone());
        cache.secret_key = Some(sk.clone());
        
        // Serialize keys
        let pk_bytes = serialize_public_key(&pk)?;
        let sk_bytes = serialize_secret_key(&sk)?;
        
        Ok((pk_bytes, sk_bytes))
    }

    pub fn create_zkp(
        &self,
        features: &[f32],
        private_key: &[u8],
    ) -> Result<ZeroKnowledgeProof> {
        // Deserialize private key if provided, otherwise use cached key
        let sk = if !private_key.is_empty() {
            deserialize_secret_key(private_key)?
        } else {
            let cache = self.kyber_cache.read().unwrap();
            cache.secret_key.as_ref()
                .ok_or_else(|| NodeError::Crypto("No secret key available".into()))?
                .clone()
        };
        
        // Use features to create a seed for randomness
        let mut feature_bytes = Vec::with_capacity(features.len() * 4);
        for &f in features {
            feature_bytes.extend_from_slice(&f.to_le_bytes());
        }
        
        // Generate commitment using Kyber encapsulation
        let (pk, _) = KyberKEM::keygen()?; // Temporary key for proof
        let (shared_secret, ct) = KyberKEM::encapsulate(&pk)?;
        
        // Create challenge using features and commitment
        let mut hasher = Sha3_256::new();
        hasher.update(&feature_bytes);
        hasher.update(&shared_secret);
        let challenge = hasher.finalize().to_vec();
        
        // Create response using ciphertext
        let response = serialize_ciphertext(&ct)?;
        
        Ok(ZeroKnowledgeProof {
            commitment: shared_secret,
            challenge,
            response,
        })
    }

    pub fn verify_zkp(
        &self,
        proof: &ZeroKnowledgeProof,
        features: &[f32],
        _public_hash: &str,
    ) -> Result<bool> {
        // Verify proof structure
        if proof.commitment.len() != 32 || proof.challenge.len() != 32 {
            return Ok(false);
        }
        
        // Reconstruct feature bytes
        let mut feature_bytes = Vec::with_capacity(features.len() * 4);
        for &f in features {
            feature_bytes.extend_from_slice(&f.to_le_bytes());
        }
        
        // Verify challenge
        let mut hasher = Sha3_256::new();
        hasher.update(&feature_bytes);
        hasher.update(&proof.commitment);
        let expected_challenge = hasher.finalize();
        
        if proof.challenge != expected_challenge.as_slice() {
            return Ok(false);
        }
        
        // Verify ciphertext
        let ct = deserialize_ciphertext(&proof.response)?;
        
        // Use cached secret key for verification
        let cache = self.kyber_cache.read().unwrap();
        if let Some(sk) = &cache.secret_key {
            let decapsulated = KyberKEM::decapsulate(sk, &ct)?;
            if decapsulated != proof.commitment {
                return Ok(false);
            }
        } else {
            return Err(NodeError::Crypto("No secret key available".into()));
        }
        
        // Update state
        let mut state = self.state.write().unwrap();
        state.current_round += 1;
        
        Ok(true)
    }

    pub fn refresh_entropy(&self) -> Result<()> {
        let mut state = self.state.write().unwrap();
        
        // Generate new entropy
        let mut new_entropy = vec![0u8; 1024];
        self.rng
            .fill(&mut new_entropy)
            .map_err(|_| NodeError::Crypto("Failed to generate entropy".into()))?;

        // Update entropy pool
        state.entropy_pool = new_entropy;
        
        // Regenerate Kyber keypair
        let (pk, sk) = KyberKEM::keygen()?;
        let mut cache = self.kyber_cache.write().unwrap();
        cache.public_key = Some(pk);
        cache.secret_key = Some(sk);
        
        Ok(())
    }

    pub fn get_algorithm_details(&self) -> Vec<QuantumAlgorithm> {
        vec![QuantumAlgorithm::Kyber]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let processor = QuantumResistantProcessor::new().unwrap();
        let (public_key, private_key) = processor.generate_keypair().unwrap();
        
        // Keys should be properly serialized now
        assert!(public_key.len() > 32);
        assert!(private_key.len() > 32);
        assert_ne!(public_key, private_key);
        
        // Verify keys can be deserialized
        assert!(deserialize_public_key(&public_key).is_ok());
        assert!(deserialize_secret_key(&private_key).is_ok());
    }

    #[test]
    fn test_zkp_creation_and_verification() {
        let processor = QuantumResistantProcessor::new().unwrap();
        let features = vec![0.1, 0.2, 0.3];
        let (_, private_key) = processor.generate_keypair().unwrap();

        let proof = processor.create_zkp(&features, &private_key).unwrap();
        let valid = processor
            .verify_zkp(&proof, &features, "test_hash")
            .unwrap();

        assert!(valid);
    }

    #[test]
    fn test_entropy_refresh() {
        let processor = QuantumResistantProcessor::new().unwrap();
        assert!(processor.refresh_entropy().is_ok());
        
        let state = processor.state.read().unwrap();
        assert_eq!(state.entropy_pool.len(), 1024);
    }

    #[test]
    fn test_invalid_proof() {
        let processor = QuantumResistantProcessor::new().unwrap();
        let features = vec![0.1, 0.2, 0.3];
        
        let invalid_proof = ZeroKnowledgeProof {
            commitment: vec![0; 32],
            challenge: vec![0; 32],
            response: vec![0; 64],
        };
        
        let result = processor.verify_zkp(&invalid_proof, &features, "test_hash").unwrap();
        assert!(!result);
    }
}
