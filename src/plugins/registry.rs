/ src/plugins/registry.rs
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<Uuid, PluginInstance>>>,
    signatures: Vec<Vec<u8>>,
}

#[derive(Debug)]
struct PluginInstance {
    plugin: Box<dyn Plugin>,
    config: PluginConfig,
    state: PluginState,
}

#[derive(Debug)]
enum PluginState {
    Inactive,
    Active,
    Failed { error: String },
}

impl PluginRegistry {
    pub async fn register(&self, plugin: Box<dyn Plugin>, config: PluginConfig) -> Result<Uuid> {
        let metadata = plugin.metadata();
        if !self.verify_signature(&metadata.signature)? {
            return Err(NodeError::Plugin("Invalid plugin signature".into()));
        }

        let instance = PluginInstance {
            plugin,
            config,
            state: PluginState::Inactive,
        };

        let mut plugins = self.plugins.write().await;
        plugins.insert(metadata.id, instance);
        Ok(metadata.id)
    }

    pub async fn initialize_all(&self) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        for instance in plugins.values_mut() {
            match instance.plugin.initialize(instance.config.clone()).await {
                Ok(_) => instance.state = PluginState::Active,
                Err(e) => instance.state = PluginState::Failed { error: e.to_string() },
            }
        }
        Ok(())
    }
}