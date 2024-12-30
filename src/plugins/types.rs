// src/plugins/types.rs
use uuid::Uuid;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub signature: Vec<u8>,
}

#[derive(Debug)]
pub struct PluginConfig {
    pub enabled: bool,
    pub path: std::path::PathBuf,
    pub settings: serde_json::Value,
}

