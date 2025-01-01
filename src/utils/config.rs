use serde::Deserialize;
use std::time::Duration;
use config::{Config as ConfigLib, ConfigError, Environment, File};
use crate::utils::error::{Result, NodeError};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub node: NodeConfig,
    pub network: NetworkConfig,
    pub storage: StorageConfig,
    pub plugins: PluginConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Deserialize)]
pub struct NodeConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub data_dir: String,
}

#[derive(Debug, Deserialize)]
pub struct NetworkConfig {
    pub use_tor: bool,
    pub peers: Vec<String>,
    pub max_connections: usize,
    pub connection_timeout: u64,
    pub heartbeat_interval: u64,
    pub peer_cleanup_interval: u64,
    pub bootstrap_nodes: Vec<String>,
    pub listen_addresses: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub path: String,
    pub encryption_key: String,
    pub max_size_gb: u64,
    pub backup_interval: u64,
    pub compression_enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct PluginConfig {
    pub enabled: bool,
    pub directory: String,
    pub allowed_origins: Vec<String>,
    pub auto_update: bool,
    pub sandbox_enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct SecurityConfig {
    pub tls_enabled: bool,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub max_request_size: usize,
    pub rate_limit_requests: u32,
    pub rate_limit_window: u64,
}

impl Config {
    pub fn new() -> Result<Self> {
        let config = ConfigLib::builder()
            // Start with default values
            .set_default("node.log_level", "info")?
            .set_default("network.max_connections", 50)?
            .set_default("network.connection_timeout", 30)?
            .set_default("network.heartbeat_interval", 60)?
            .set_default("network.peer_cleanup_interval", 300)?
            .set_default("storage.max_size_gb", 10)?
            .set_default("storage.backup_interval", 86400)?
            .set_default("storage.compression_enabled", true)?
            .set_default("plugins.enabled", true)?
            .set_default("plugins.auto_update", false)?
            .set_default("plugins.sandbox_enabled", true)?
            .set_default("security.tls_enabled", false)?
            .set_default("security.max_request_size", 10_485_760)?  // 10MB
            .set_default("security.rate_limit_requests", 100)?
            .set_default("security.rate_limit_window", 60)?
            
            // Load from config file
            .add_source(File::with_name("config/default"))
            .add_source(File::with_name("config/local").required(false))
            
            // Override with environment variables (e.g., APP_NODE_HOST)
            .add_source(Environment::with_prefix("APP").separator("_"))
            
            .build()?;

        let config: Self = config.try_deserialize()?;
        config.validate()?;
        
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        // Validate node configuration
        if self.node.port == 0 {
            return Err(NodeError::Config("Invalid port number".into()));
        }

        // Validate network configuration
        if self.network.max_connections == 0 {
            return Err(NodeError::Config("max_connections must be greater than 0".into()));
        }
        if self.network.peers.is_empty() && self.network.bootstrap_nodes.is_empty() {
            return Err(NodeError::Config("No peers or bootstrap nodes configured".into()));
        }

        // Validate storage configuration
        if self.storage.max_size_gb == 0 {
            return Err(NodeError::Config("max_size_gb must be greater than 0".into()));
        }
        if self.storage.encryption_key.is_empty() {
            return Err(NodeError::Config("encryption_key must be set".into()));
        }

        // Validate security configuration
        if self.security.tls_enabled {
            if self.security.tls_cert_path.is_none() || self.security.tls_key_path.is_none() {
                return Err(NodeError::Config("TLS cert and key paths must be set when TLS is enabled".into()));
            }
        }

        Ok(())
    }

    pub fn get_connection_timeout(&self) -> Duration {
        Duration::from_secs(self.network.connection_timeout)
    }

    pub fn get_heartbeat_interval(&self) -> Duration {
        Duration::from_secs(self.network.heartbeat_interval)
    }

    pub fn get_peer_cleanup_interval(&self) -> Duration {
        Duration::from_secs(self.network.peer_cleanup_interval)
    }

    pub fn get_backup_interval(&self) -> Duration {
        Duration::from_secs(self.storage.backup_interval)
    }
}

impl From<ConfigError> for NodeError {
    fn from(error: ConfigError) -> Self {
        NodeError::Config(error.to_string())
    }
}
