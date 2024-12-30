// src/plugins/manager.rs
use std::collections::HashMap;
use libloading::{Library, Symbol};

pub struct PluginManager {
    plugins: HashMap<Uuid, Box<dyn Plugin>>,
    libraries: Vec<Library>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            libraries: Vec::new(),
        }
    }

    pub async fn load_plugin(&mut self, path: &std::path::Path) -> Result<Uuid> {
        unsafe {
            let lib = Library::new(path)?;
            let create_plugin: Symbol<fn() -> Box<dyn Plugin>> = 
                lib.get(b"_create_plugin")?;
            
            let mut plugin = create_plugin();
            let metadata = plugin.metadata().clone();
            let id = metadata.id;
            
            plugin.initialize(PluginConfig {
                enabled: true,
                path: path.to_path_buf(),
                settings: serde_json::Value::Null,
            }).await?;
            
            self.plugins.insert(id, plugin);
            self.libraries.push(lib);
            
            Ok(id)
        }
    }
}

