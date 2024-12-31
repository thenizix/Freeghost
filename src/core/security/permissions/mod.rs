// src/core/security/permissions/mod.rs
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use sha3::{Sha3_256, Digest};
use uuid::Uuid;
use tracing::{info, warn, error};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Permission {
    id: Uuid,
    name: String,
    scope: PermissionScope,
    level: PermissionLevel,
    requires: HashSet<Uuid>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum PermissionScope {
    System,
    Biometric,
    Network,
    Storage,
    Plugin(String),
    Custom(String),
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum PermissionLevel {
    Admin,
    Manager,
    User,
    Guest,
}

#[derive(Debug, Clone)]
pub struct Role {
    id: Uuid,
    name: String,
    permissions: HashSet<Uuid>,
    inherits: HashSet<Uuid>,
}

pub struct PermissionManager {
    permissions: Arc<RwLock<HashMap<Uuid, Permission>>>,
    roles: Arc<RwLock<HashMap<Uuid, Role>>>,
    assignments: Arc<RwLock<HashMap<String, HashSet<Uuid>>>>, // entity_id -> role_ids
    cache: Arc<RwLock<PermissionCache>>,
}

#[derive(Debug, Default)]
struct PermissionCache {
    entity_permissions: HashMap<String, HashSet<Uuid>>,
    last_updated: std::time::SystemTime,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self {
            permissions: Arc::new(RwLock::new(HashMap::new())),
            roles: Arc::new(RwLock::new(HashMap::new())),
            assignments: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(PermissionCache::default())),
        }
    }

    pub async fn create_permission(
        &self,
        name: String,
        scope: PermissionScope,
        level: PermissionLevel,
        requires: HashSet<Uuid>,
    ) -> Result<Uuid, PermissionError> {
        // Validate requirements
        let permissions = self.permissions.read().await;
        for req_id in &requires {
            if !permissions.contains_key(req_id) {
                return Err(PermissionError::InvalidRequirement(*req_id));
            }
        }

        let permission = Permission {
            id: Uuid::new_v4(),
            name,
            scope,
            level,
            requires,
        };

        // Store permission
        drop(permissions);
        self.permissions.write().await.insert(permission.id, permission.clone());
        
        // Invalidate cache
        self.invalidate_cache().await;

        Ok(permission.id)
    }

    pub async fn create_role(
        &self,
        name: String,
        permissions: HashSet<Uuid>,
        inherits: HashSet<Uuid>,
    ) -> Result<Uuid, PermissionError> {
        // Validate permissions and inheritance
        self.validate_role_permissions(&permissions).await?;
        self.validate_role_inheritance(&inherits).await?;

        let role = Role {
            id: Uuid::new_v4(),
            name,
            permissions,
            inherits,
        };

        // Store role
        self.roles.write().await.insert(role.id, role.clone());
        
        // Invalidate cache
        self.invalidate_cache().await;

        Ok(role.id)
    }

    pub async fn assign_role(
        &self,
        entity_id: String,
        role_id: Uuid,
    ) -> Result<(), PermissionError> {
        let roles = self.roles.read().await;
        if !roles.contains_key(&role_id) {
            return Err(PermissionError::InvalidRole(role_id));
        }

        let mut assignments = self.assignments.write().await;
        assignments
            .entry(entity_id.clone())
            .or_insert_with(HashSet::new)
            .insert(role_id);

        // Invalidate cache
        self.invalidate_cache().await;

        info!("Assigned role {} to entity {}", role_id, entity_id);
        Ok(())
    }

    pub async fn check_permission(
        &self,
        entity_id: &str,
        permission_id: Uuid,
    ) -> Result<bool, PermissionError> {
        // Check cache first
        if let Some(has_permission) = self.check_cache(entity_id, permission_id).await {
            return Ok(has_permission);
        }

        let permissions = self.permissions.read().await;
        let roles = self.roles.read().await;
        let assignments = self.assignments.read().await;

        // Get entity's roles
        let entity_roles = if let Some(role_ids) = assignments.get(entity_id) {
            role_ids
        } else {
            return Ok(false);
        };

        // Check each role
        for &role_id in entity_roles {
            if let Some(role) = roles.get(&role_id) {
                if self.role_has_permission(role, permission_id, &roles, &permissions).await {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    async fn role_has_permission(
        &self,
        role: &Role,
        permission_id: Uuid,
        roles: &HashMap<Uuid, Role>,
        permissions: &HashMap<Uuid, Permission>,
    ) -> bool {
        // Direct permission check
        if role.permissions.contains(&permission_id) {
            return true;
        }

        // Inherited permissions check
        for &inherited_role_id in &role.inherits {
            if let Some(inherited_role) = roles.get(&inherited_role_id) {
                if self.role_has_permission(inherited_role, permission_id, roles, permissions).await {
                    return true;
                }
            }
        }

        false
    }

    async fn validate_role_permissions(
        &self,
        permissions: &HashSet<Uuid>,
    ) -> Result<(), PermissionError> {
        let stored_permissions = self.permissions.read().await;
        for &permission_id in permissions {
            if !stored_permissions.contains_key(&permission_id) {
                return Err(PermissionError::InvalidPermission(permission_id));
            }
        }
        Ok(())
    }

    async fn validate_role_inheritance(
        &self,
        inherits: &HashSet<Uuid>,
    ) -> Result<(), PermissionError> {
        let roles = self.roles.read().await;
        for &role_id in inherits {
            if !roles.contains_key(&role_id) {
                return Err(PermissionError::InvalidRole(role_id));
            }
        }
        Ok(())
    }

    async fn check_cache(&self, entity_id: &str, permission_id: Uuid) -> Option<bool> {
        let cache = self.cache.read().await;
        cache.entity_permissions
            .get(entity_id)
            .map(|permissions| permissions.contains(&permission_id))
    }

    async fn invalidate_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.entity_permissions.clear();
        cache.last_updated = std::time::SystemTime::now();
    }

    pub async fn revoke_role(
        &self,
        entity_id: &str,
        role_id: Uuid,
    ) -> Result<(), PermissionError> {
        let mut assignments = self.assignments.write().await;
        if let Some(roles) = assignments.get_mut(entity_id) {
            roles.remove(&role_id);
            self.invalidate_cache().await;
            info!("Revoked role {} from entity {}", role_id, entity_id);
            Ok(())
        } else {
            Err(PermissionError::EntityNotFound(entity_id.to_string()))
        }
    }

    pub async fn list_entity_permissions(
        &self,
        entity_id: &str,
    ) -> Result<HashSet<Permission>, PermissionError> {
        let permissions = self.permissions.read().await;
        let roles = self.roles.read().await;
        let assignments = self.assignments.read().await;

        let mut entity_permissions = HashSet::new();

        if let Some(role_ids) = assignments.get(entity_id) {
            for &role_id in role_ids {
                if let Some(role) = roles.get(&role_id) {
                    for &permission_id in &role.permissions {
                        if let Some(permission) = permissions.get(&permission_id) {
                            entity_permissions.insert(permission.clone());
                        }
                    }
                }
            }
        }

        Ok(entity_permissions)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("Invalid permission ID: {0}")]
    InvalidPermission(Uuid),
    #[error("Invalid role ID: {0}")]
    InvalidRole(Uuid),
    #[error("Invalid permission requirement: {0}")]
    InvalidRequirement(Uuid),
    #[error("Entity not found: {0}")]
    EntityNotFound(String),
    #[error("Permission cycle detected")]
    CycleDetected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_permission_creation_and_validation() {
        let manager = PermissionManager::new();
        
        // Create a basic permission
        let perm_id = manager.create_permission(
            "read_data".to_string(),
            PermissionScope::Storage,
            PermissionLevel::User,
            HashSet::new(),
        ).await.unwrap();

        // Create a role with the permission
        let role_id = manager.create_role(
            "data_reader".to_string(),
            HashSet::from([perm_id]),
            HashSet::new(),
        ).await.unwrap();

        // Assign role to entity
        manager.assign_role("user1".to_string(), role_id).await.unwrap();

        // Verify permission
        assert!(manager.check_permission("user1", perm_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_role_inheritance() {
        let manager = PermissionManager::new();

        // Create base permission
        let base_perm = manager.create_permission(
            "base".to_string(),
            PermissionScope::System,
            PermissionLevel::User,
            HashSet::new(),
        ).await.unwrap();

        // Create base role
        let base_role = manager.create_role(
            "base_role".to_string(),
            HashSet::from([base_perm]),
            HashSet::new(),
        ).await.unwrap();

        // Create inheriting role
        let inherited_role = manager.create_role(
            "inherited_role".to_string(),
            HashSet::new(),
            HashSet::from([base_role]),
        ).await.unwrap();

        // Assign inherited role to entity
        manager.assign_role("user2".to_string(), inherited_role).await.unwrap();

        // Verify inherited permission
        assert!(manager.check_permission("user2", base_perm).await.unwrap());
    }
}