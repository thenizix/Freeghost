// src/network/protocols/mod.rs
pub mod tor;
pub mod p2p;
pub mod fallback;

use async_trait::async_trait;
use crate::core::identity::Template;

#[async_trait]
pub trait NetworkProtocol: Send + Sync {
    async fn broadcast(&self, message: NetworkMessage) -> Result<()>;
    async fn receive(&self) -> Result<NetworkMessage>;
    async fn verify_template(&self, template: &Template) -> Result<bool>;
}