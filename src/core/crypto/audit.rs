// src/core/crypto/audit.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use super::types::{SecurityLevel, CryptoMetadata};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("Failed to record audit event")]
    RecordingFailed,
    #[error("Failed to retrieve audit logs")]
    RetrievalFailed,
    #[error("Invalid audit period")]
    InvalidPeriod,
    #[error("Storage error: {0}")]
    StorageError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    KeyGeneration,
    KeyRotation,
    SignatureCreation,
    SignatureVerification,
    TemplateGeneration,
    TemplateVerification,
    SecurityLevelChange,
    AuthenticationAttempt { success: bool },
    AnomalyDetected { severity: AnomalySeverity },
    SystemStartup,
    SystemShutdown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub event_type: AuditEventType,
    pub timestamp: DateTime<Utc>,
    pub security_level: SecurityLevel,
    pub component_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_events: usize,
    pub events_by_type: std::collections::HashMap<String, usize>,
    pub anomalies_detected: usize,
    pub security_level_changes: usize,
}

pub struct AuditSystem {
    storage: Arc<RwLock<AuditStorage>>,
    retention_period: chrono::Duration,
    current_session: Uuid,
    metadata: CryptoMetadata,
}

struct AuditStorage {
    events: Vec<AuditEvent>,
    index: std::collections::BTreeMap<DateTime<Utc>, Vec<usize>>,
}

impl AuditSystem {
    pub fn new(retention_days: i64, security_level: SecurityLevel) -> Self {
        let storage = Arc::new(RwLock::new(AuditStorage {
            events: Vec::new(),
            index: std::collections::BTreeMap::new(),
        }));

        let system = Self {
            storage,
            retention_period: chrono::Duration::days(retention_days),
            current_session: Uuid::new_v4(),
            metadata: CryptoMetadata::new(security_level),
        };

        // Record system startup
        tokio::spawn(async move {
            let _ = system.record_event(
                AuditEventType::SystemStartup,
                None,
                None,
            ).await;
        });

        system
    }

    pub async fn record_event(
        &self,
        event_type: AuditEventType,
        component_id: Option<Uuid>,
        metadata: Option<serde_json::Value>,
    ) -> Result<Uuid, AuditError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            event_type,
            timestamp: Utc::now(),
            security_level: self.metadata.security_level,
            component_id,
            metadata,
            session_id: Some(self.current_session),
        };

        let mut storage = self.storage.write().await;
        
        // Add to main storage
        let event_index = storage.events.len();
        storage.events.push(event.clone());

        // Update time index
        storage.index
            .entry(event.timestamp.date().and_hms(0, 0, 0))
            .or_insert_with(Vec::new)
            .push(event_index);

        // Cleanup old events
        self.cleanup_old_events(&mut storage).await;

        Ok(event.id)
    }

    pub async fn get_events(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        if end_time < start_time {
            return Err(AuditError::InvalidPeriod);
        }

        let storage = self.storage.read().await;
        let mut events = Vec::new();

        for (date, indices) in storage.index.range(start_time..=end_time) {
            if date >= &start_time && date <= &end_time {
                for &index in indices {
                    if let Some(event) = storage.events.get(index) {
                        if event.timestamp >= start_time && event.timestamp <= end_time {
                            events.push(event.clone());
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    pub async fn get_summary(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<AuditSummary, AuditError> {
        let events = self.get_events(start_time, end_time).await?;
        let mut events_by_type = std::collections::HashMap::new();
        let mut anomalies_detected = 0;
        let mut security_level_changes = 0;

        for event in &events {
            let type_name = format!("{:?}", event.event_type);
            *events_by_type.entry(type_name).or_insert(0) += 1;

            match &event.event_type {
                AuditEventType::AnomalyDetected { .. } => anomalies_detected += 1,
                AuditEventType::SecurityLevelChange => security_level_changes += 1,
                _ => {}
            }
        }

        Ok(AuditSummary {
            period_start: start_time,
            period_end: end_time,
            total_events: events.len(),
            events_by_type,
            anomalies_detected,
            security_level_changes,
        })
    }

    async fn cleanup_old_events(&self, storage: &mut AuditStorage) {
        let cutoff = Utc::now() - self.retention_period;
        
        // Remove old events
        storage.events.retain(|event| event.timestamp >= cutoff);
        
        // Update index
        storage.index.retain(|date, indices| {
            if date < &cutoff.date().and_hms(0, 0, 0) {
                false
            } else {
                // Update indices to account for removed events
                indices.retain(|&index| index < storage.events.len());
                !indices.is_empty()
            }
        });
    }

    pub fn get_current_session(&self) -> Uuid {
        self.current_session
    }
}

impl Drop for AuditSystem {
    fn drop(&mut self) {
        // Record system shutdown
        let shutdown_event = AuditEvent {
            id: Uuid::new_v4(),
            event_type: AuditEventType::SystemShutdown,
            timestamp: Utc::now(),
            security_level: self.metadata.security_level,
            component_id: None,
            metadata: None,
            session_id: Some(self.current_session),
        };

        if let Ok(mut storage) = futures::executor::block_on(self.storage.write()) {
            storage.events.push(shutdown_event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audit_event_recording() {
        let audit_system = AuditSystem::new(30, SecurityLevel::Standard);
        
        let event_id = audit_system.record_event(
            AuditEventType::KeyGeneration,
            Some(Uuid::new_v4()),
            None,
        ).await.unwrap();
        
        let events = audit_system.get_events(
            Utc::now() - chrono::Duration::hours(1),
            Utc::now(),
        ).await.unwrap();
        
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| e.id == event_id));
    }

    #[tokio::test]
    async fn test_audit_summary() {
        let audit_system = AuditSystem::new(30, SecurityLevel::Standard);
        
        // Record multiple events
        for _ in 0..5 {
            audit_system.record_event(
                AuditEventType::KeyGeneration,
                None,
                None,
            ).await.unwrap();
        }
        
        audit_system.record_event(
            AuditEventType::AnomalyDetected { severity: AnomalySeverity::High },
            None,
            None,
        ).await.unwrap();
        
        let summary = audit_system.get_summary(
            Utc::now() - chrono::Duration::hours(1),
            Utc::now(),
        ).await.unwrap();
        
        assert_eq!(summary.total_events, 6);
        assert_eq!(summary.anomalies_detected, 1);
    }

    #[tokio::test]
    async fn test_retention_period() {
        let audit_system = AuditSystem::new(1, SecurityLevel::Standard);
        
        // Record old event
        let old_time = Utc::now() - chrono::Duration::days(2);
        let mut storage = audit_system.storage.write().await;
        storage.events.push(AuditEvent {
            id: Uuid::new_v4(),
            event_type: AuditEventType::KeyGeneration,
            timestamp: old_time,
            security_level: SecurityLevel::Standard,
            component_id: None,
            metadata: None,
            session_id: None,
        });
        
        // Trigger cleanup
        audit_system.cleanup_old_events(&mut storage).await;
        
        // Verify old event was removed
        let events = audit_system.get_events(
            old_time,
            Utc::now(),
        ).await.unwrap();
        
        assert!(events.is_empty());
    }
}