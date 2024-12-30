// src/plugins/traits/mod.rs
#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;
    async fn initialize(&mut self, config: PluginConfig) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}

