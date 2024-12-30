// src/utils/config.rs
use serde::Deserialize;
use config::{Config as ConfigLib, ConfigError, File};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub node: NodeConfig,
    pub network: NetworkConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Deserialize)]
pub struct NodeConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct NetworkConfig {
    pub use_tor: bool,
    pub peers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub path: String,
    pub encryption_key: String,
}

impl Config {
    pub fn new() -> Result<Self> {
        let config = ConfigLib::builder()
            .add_source(File::with_name("config/node_config"))
            .build()?;
            
        config.try_deserialize()
            .map_err(|e| NodeError::Config(e.to_string()))
    }
}