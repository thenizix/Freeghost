//! Cryptographic primitives and implementations

pub mod key_manager;
pub mod quantum;
pub mod kyber;
pub mod ntt;
pub mod sampling;
pub mod serialization;

// Re-export commonly used types
pub use kyber::{KyberKEM, PublicKey, SecretKey, Ciphertext};
pub use ntt::NTTContext;
pub use serialization::{
    serialize_public_key, deserialize_public_key,
    serialize_secret_key, deserialize_secret_key,
    serialize_ciphertext, deserialize_ciphertext,
};
