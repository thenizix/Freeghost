// src/plugins/versioning.rs
pub struct VersionManager {
    min_version: semver::Version,
    compatibility_map: HashMap<String, Vec<semver::VersionReq>>,
}

impl VersionManager {
    pub fn check_compatibility(&self, plugin: &dyn Plugin) -> Result<()> {
        let metadata = plugin.metadata();
        let version = semver::Version::parse(&metadata.version)?;
        
        if version < self.min_version {
            return Err(NodeError::Plugin("Plugin version too old".into()));
        }

        if let Some(reqs) = self.compatibility_map.get(&metadata.name) {
            for req in reqs {
                if !req.matches(&version) {
                    return Err(NodeError::Plugin("Incompatible plugin version".into()));
                }
            }
        }
        Ok(())
    }
}