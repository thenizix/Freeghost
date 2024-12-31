// src/network/transport/message.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    pub id: Uuid,
    pub message_type: MessageType,
    pub payload: Vec<u8>,
    pub timestamp: i64,
    pub sender: String,
    pub recipient: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    IdentityVerification,
    TemplateUpdate,
    KeyRotation,
    HealthCheck,
    Custom(String),
}
