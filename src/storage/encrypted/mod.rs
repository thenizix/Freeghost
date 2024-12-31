// src/storage/encrypted/mod.rs
mod store;
mod cipher;
mod errors;

pub use store::EncryptedStore;
pub use errors::StorageError;