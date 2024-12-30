// src/plugins/loader.rs
use libloading::{Library, Symbol};

pub struct PluginLoader {
    registry: Arc<PluginRegistry>,
    loaded_libraries: Vec<Library>,
}

impl PluginLoader {
    pub async fn load_from_directory(&mut self, dir: &Path) -> Result<Vec<Uuid>> {
        let mut loaded_ids = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().map_or(false, |ext| ext == "so") {
                if let Ok(id) = self.load_plugin(&path).await {
                    loaded_ids.push(id);
                }
            }
        }
        Ok(loaded_ids)
    }

    async fn load_plugin(&mut self, path: &Path) -> Result<Uuid> {
        unsafe {
            let library = Library::new(path)?;
            let create_fn: Symbol<CreatePluginFn> = library.get(b"_create_plugin")?;
            let plugin = create_fn()?;
            
            let config = PluginConfig {
                enabled: true,
                path: path.to_path_buf(),
                settings: self.load_plugin_settings(path)?,
            };

            let id = self.registry.register(plugin, config).await?;
            self.loaded_libraries.push(library);
            Ok(id)
        }
    }
}
